/*
 * @file focus.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Window focus monitor and control, X11 backend
 * @date 2026-09-26
 *
 * @copyright Copyright (c) 2026
 */

use std::time::Duration;

use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, EventMask, InputFocus,
    StackMode, Timestamp, Window,
};
use x11rb::rust_connection::RustConnection;

use crate::error::Result;

// Per-file logger id, same scheme as in main.rs.
const LOG_ID: &str = "[PlatformFocus]";

/// A window as X11 identifies it, so that callers can talk about "the window the text goes back
/// to" without importing the protocol crate themselves.
pub type WindowId = Window;

/// `CurrentTime`: the server substitutes the time of the request itself. Spelled out because
/// x11rb only names the value for the 8-bit `Time` type, not for `Timestamp`.
const CURRENT_TIME: Timestamp = 0;

/// `XGetInputFocus` answers with these two instead of a window when the keyboard is not in a
/// real window: `None` (nothing is focused) and `PointerRoot` (the pointer decides). Neither
/// can be handed text later on.
const FOCUS_NONE: WindowId = 0;
const FOCUS_POINTER_ROOT: WindowId = 1;

/// How far up the window tree to walk before giving up. A child window sits one or two levels
/// below its top level; the bound only exists so that a cycle cannot hang the caller.
const MAX_PARENT_HOPS: usize = 16;

/// EWMH source indication meaning "the user asked for this directly", as opposed to another
/// application doing it programmatically.
const ACTIVE_WINDOW_SOURCE_USER: u32 = 2;

/// The two halves of an EWMH state change: the mode that adds a state, and the source indication
/// that marks the request as coming from the application itself rather than from a pager.
const WM_STATE_ADD: u32 = 1;
const WM_STATE_SOURCE_APPLICATION: u32 = 1;

/// How often giving the keyboard to a window is retried, and how long to wait in between: right
/// after being shown, the window is not mapped yet and the server refuses to focus it.
const FOCUS_ATTEMPTS: usize = 10;
const FOCUS_RETRY_PAUSE: Duration = Duration::from_millis(20);

/// How long to wait after [`Focus::activate`] before typing into the window.
///
/// The activation is a request to the window manager, and keys sent before it has processed that
/// request land wherever the focus still was — the panel, the terminal that started the process,
/// anything. Better to wait one frame too long than to lose the text.
pub const ACTIVATION_PAUSE: Duration = Duration::from_millis(60);

/// The X11 window focus.
///
/// Two directions, both needed by the flow: reading which window has the keyboard *before* the
/// panel is shown, and giving the keyboard back to it once the user has picked a word. Only X11
/// is covered — under Wayland there is no global focus to read at all.
pub struct Focus {
    connection: RustConnection,
    /// The root window: `_NET_ACTIVE_WINDOW` lives there, and client messages are sent there.
    root: WindowId,
    /// `_NET_ACTIVE_WINDOW`, the property an EWMH window manager keeps up to date.
    active_window_atom: Atom,
    /// `WM_STATE`, which the ICCCM sets on managed windows only: how a top level is told apart
    /// from a child window of some application.
    wm_state_atom: Atom,
    /// `_NET_SUPPORTING_WM_CHECK`, which an EWMH window manager announces itself with.
    wm_check_atom: Atom,
    /// `_NET_WM_STATE`, the property a client changes to make the window manager keep a window
    /// above the others, and `_NET_WM_STATE_ABOVE`, the state that means exactly that.
    net_wm_state_atom: Atom,
    above_atom: Atom,
    /// `_NET_CLIENT_LIST`, the windows the window manager knows about — our own panel is found
    /// through it — and `_NET_CLIENT_LIST_STACKING`, the same windows in stacking order.
    client_list_atom: Atom,
    stacking_atom: Atom,
    /// `_NET_WM_NAME`, the title the window manager reports for a window.
    window_name_atom: Atom,
}

impl Focus {
    /// Connects to the X server `DISPLAY` names. The atoms are interned once here so that the
    /// per-event path costs no round trip beyond the requests it really needs.
    pub fn connect() -> Result<Self> {
        let (connection, screen) = x11rb::connect(None)?;
        let root = connection.setup().roots[screen].root;

        let active_window_atom = intern(&connection, b"_NET_ACTIVE_WINDOW")?;
        let wm_state_atom = intern(&connection, b"WM_STATE")?;
        let wm_check_atom = intern(&connection, b"_NET_SUPPORTING_WM_CHECK")?;
        let net_wm_state_atom = intern(&connection, b"_NET_WM_STATE")?;
        let above_atom = intern(&connection, b"_NET_WM_STATE_ABOVE")?;
        let client_list_atom = intern(&connection, b"_NET_CLIENT_LIST")?;
        let stacking_atom = intern(&connection, b"_NET_CLIENT_LIST_STACKING")?;
        let window_name_atom = intern(&connection, b"_NET_WM_NAME")?;

        info!(target: LOG_ID, "Connected to the X server, root window 0x{root:x}.");
        Ok(Self {
            connection,
            root,
            active_window_atom,
            wm_state_atom,
            wm_check_atom,
            net_wm_state_atom,
            above_atom,
            client_list_atom,
            stacking_atom,
            window_name_atom,
        })
    }

    /// The top level window the keyboard focus is in, or `None` when nothing is focused.
    ///
    /// Call this *before* showing the panel: showing it moves the focus to the panel, and from
    /// then on this only ever answers with our own window. The window manager is asked first,
    /// because it knows the top level window directly; the server is the fallback, and what it
    /// reports is usually a child window that then has to be walked up.
    pub fn current(&self) -> Result<Option<WindowId>> {
        if let Some(window) = self.active_window()? {
            debug!(target: LOG_ID, "The window manager has 0x{window:x} active.");
            return Ok(Some(window));
        }

        let focused = self.connection.get_input_focus()?.reply()?.focus;
        if focused == FOCUS_NONE || focused == FOCUS_POINTER_ROOT {
            debug!(target: LOG_ID, "The server reports no focused window.");
            return Ok(None);
        }

        let top = self.top_level(focused)?;
        debug!(target: LOG_ID, "The server has 0x{focused:x} focused, top level 0x{top:x}.");
        Ok(Some(top))
    }

    /// Gives the keyboard focus to `window`, a top level window captured earlier.
    ///
    /// With an EWMH window manager this is a `_NET_ACTIVE_WINDOW` client message to the root
    /// window: the window manager raises and focuses the window itself, which is the only way it
    /// does not immediately undo. Without one, `SetInputFocus` points the focus at the window
    /// directly. Either way the change is asynchronous — wait [`ACTIVATION_PAUSE`] before typing.
    pub fn activate(&self, window: WindowId) -> Result<()> {
        if self.has_ewmh_window_manager()? {
            // data.l[0] source indication, data.l[1] timestamp, data.l[2] the requestor's own
            // active window (none: we are not a pager), the rest unused.
            let event = ClientMessageEvent::new(
                32,
                window,
                self.active_window_atom,
                [
                    ACTIVE_WINDOW_SOURCE_USER,
                    CURRENT_TIME,
                    x11rb::NONE,
                    x11rb::NONE,
                    x11rb::NONE,
                ],
            );

            self.connection
                .send_event(
                    false,
                    self.root,
                    EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                    event,
                )?
                .check()?;
        } else {
            warn!(target: LOG_ID, "No EWMH window manager, asking the server for the focus directly.");
            self.connection
                .set_input_focus(InputFocus::PARENT, window, CURRENT_TIME)?
                .check()?;
        }

        // Requests are buffered: without the flush the focus change can still be sitting in our
        // own buffer while the text is already being typed.
        self.connection.flush()?;
        info!(target: LOG_ID, "Gave the keyboard focus to 0x{window:x}.");
        Ok(())
    }

    /// Finds a window by the title the window manager reports for it.
    ///
    /// This is how the panel's *own* window is found: Slint keeps its window handle to itself, and
    /// the title is the only name the X server knows that window by (it is set in
    /// ui/app_window.slint). The search covers the windows the window manager manages, which is
    /// where a panel that is on screen shows up.
    pub fn find_window_by_title(&self, title: &str) -> Result<Option<WindowId>> {
        let Some(windows) = self.client_list()? else {
            return Ok(None);
        };

        for window in windows {
            let reply = self
                .connection
                .get_property(false, window, self.window_name_atom, AtomEnum::ANY, 0, 1024)?
                .reply()?;
            if reply.value == title.as_bytes() {
                return Ok(Some(window));
            }
        }

        Ok(None)
    }

    /// Whether `window` is the topmost window on screen, read from the window manager's stacking
    /// order: the last window in that list is the one on top.
    ///
    /// The stacking order is the only reliable answer to "is it in front" — the keyboard focus is
    /// not, a window can hold the focus while another one sits above it.
    pub fn is_frontmost(&self, window: WindowId) -> Result<bool> {
        let reply = self
            .connection
            .get_property(
                false,
                self.root,
                self.stacking_atom,
                AtomEnum::WINDOW,
                0,
                1024,
            )?
            .reply()?;

        Ok(reply
            .value32()
            .is_some_and(|windows| windows.last() == Some(window)))
    }

    /// Puts `window` in front of the other windows and gives it the keyboard.
    ///
    /// Asking the window manager for the `_NET_WM_STATE_ABOVE` state, rather than for activation,
    /// is deliberate: GNOME refuses an activation request that cannot point at a user interaction,
    /// which is exactly what a window opened by a global hotkey looks like. For the same reason the
    /// keyboard is pointed at the window directly, retried until the server has mapped it — a
    /// window that was just shown is not there to be focused yet.
    pub fn bring_to_front(&self, window: WindowId) -> Result<()> {
        self.keep_above(window)?;
        // The state above only takes effect through the window manager, which may need a moment or
        // may not be asked at all; raising the window stacks it on top right now, on our side.
        self.connection
            .configure_window(
                window,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?
            .check()?;
        self.give_keyboard(window)?;
        self.connection.flush()?;
        info!(target: LOG_ID, "Put 0x{window:x} in front.");
        Ok(())
    }

    /// Reads `_NET_ACTIVE_WINDOW` from the root window. `None` means the property is not there:
    /// no EWMH window manager, or nothing is active.
    fn active_window(&self) -> Result<Option<WindowId>> {
        let reply = self
            .connection
            .get_property(
                false,
                self.root,
                self.active_window_atom,
                AtomEnum::WINDOW,
                0,
                1,
            )?
            .reply()?;

        let Some(mut values) = reply.value32() else {
            return Ok(None);
        };
        match values.next() {
            Some(window) if window != x11rb::NONE => Ok(Some(window)),
            _ => Ok(None),
        }
    }

    /// Walks up from `window` to the window the window manager manages.
    ///
    /// That is the window to activate later: pointing the focus at a child of a foreign
    /// application is refused, and would not survive the application's own focus handling.
    fn top_level(&self, window: WindowId) -> Result<WindowId> {
        let mut current = window;

        for _ in 0..MAX_PARENT_HOPS {
            if self.has_wm_state(current)? {
                return Ok(current);
            }

            let parent = self.connection.query_tree(current)?.reply()?.parent;
            // The root window has no parent and carries no WM_STATE: stop there rather than
            // walking past the window the user was actually in.
            if parent == x11rb::NONE || parent == self.root {
                return Ok(current);
            }

            current = parent;
        }

        warn!(target: LOG_ID, "No top level window within {MAX_PARENT_HOPS} levels of 0x{window:x}, using 0x{current:x}.");
        Ok(current)
    }

    /// Whether `window` is managed by the window manager, which is what `WM_STATE` marks. The
    /// property is read with `AnyPropertyType` and its type compared against `None`, which is
    /// what a missing property reads as.
    fn has_wm_state(&self, window: WindowId) -> Result<bool> {
        let reply = self
            .connection
            .get_property(false, window, self.wm_state_atom, AtomEnum::ANY, 0, 0)?
            .reply()?;

        Ok(reply.type_ != x11rb::NONE)
    }

    /// Whether an EWMH window manager is running: it announces itself by putting
    /// `_NET_SUPPORTING_WM_CHECK` on the root window.
    fn has_ewmh_window_manager(&self) -> Result<bool> {
        let reply = self
            .connection
            .get_property(false, self.root, self.wm_check_atom, AtomEnum::WINDOW, 0, 1)?
            .reply()?;

        let Some(mut values) = reply.value32() else {
            return Ok(false);
        };
        Ok(matches!(values.next(), Some(window) if window != x11rb::NONE))
    }

    /// Adds `_NET_WM_STATE_ABOVE` to `window`, as the EWMH state-change client message. Adding a
    /// state the window already has is a no-op for the window manager, so sending it again on every
    /// show is safe — and necessary, because Slint only asks for it once, when the level changes.
    fn keep_above(&self, window: WindowId) -> Result<()> {
        let event = ClientMessageEvent::new(
            32,
            window,
            self.net_wm_state_atom,
            [
                WM_STATE_ADD,
                self.above_atom,
                x11rb::NONE,
                WM_STATE_SOURCE_APPLICATION,
                x11rb::NONE,
            ],
        );

        self.connection
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )?
            .check()?;
        Ok(())
    }

    /// Points the keyboard at `window`, retrying while the server still refuses — which it does
    /// until the window is mapped — and then checks that the focus actually arrived there.
    fn give_keyboard(&self, window: WindowId) -> Result<()> {
        let mut refusal = None;

        for _ in 0..FOCUS_ATTEMPTS {
            match self
                .connection
                .set_input_focus(InputFocus::PARENT, window, CURRENT_TIME)?
                .check()
            {
                Ok(()) => {
                    refusal = None;
                    break;
                }
                Err(error) => {
                    refusal = Some(error);
                    std::thread::sleep(FOCUS_RETRY_PAUSE);
                }
            }
        }

        let Some(error) = refusal else {
            self.connection.flush()?;

            // The window manager can disagree about who should have the keyboard and take it back;
            // this line is what tells us that happened, out on a real session.
            let focused = self.connection.get_input_focus()?.reply()?.focus;
            if focused == window {
                debug!(target: LOG_ID, "The keyboard is on 0x{window:x}.");
            } else {
                warn!(target: LOG_ID, "Asked for the keyboard on 0x{window:x}, but 0x{focused:x} has it.");
            }

            return Ok(());
        };

        // Out of attempts: the window never became focusable.
        Err(error.into())
    }

    /// The windows the window manager manages, in the order it reports them.
    fn client_list(&self) -> Result<Option<Vec<WindowId>>> {
        let reply = self
            .connection
            .get_property(
                false,
                self.root,
                self.client_list_atom,
                AtomEnum::WINDOW,
                0,
                1024,
            )?
            .reply()?;

        Ok(reply.value32().map(|windows| windows.collect()))
    }
}

/// Resolves one atom. `only_if_exists` is false on purpose: an atom the window manager has not
/// created yet is created here, and an atom nobody uses costs nothing.
fn intern(connection: &RustConnection, name: &[u8]) -> Result<Atom> {
    Ok(connection.intern_atom(false, name)?.reply()?.atom)
}
