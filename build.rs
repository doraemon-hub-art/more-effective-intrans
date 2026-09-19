/*
 * @file build.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Pre-compile the Slint DSL sources into Rust code
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

// The Rust counterpart of Qt's moc/uic: cargo runs this before compiling the crate.
// It parses ui/app_window.slint and writes the generated Rust module into OUT_DIR,
// which the crate then pulls in with slint::include_modules!().
fn main() {
    slint_build::compile("ui/app_window.slint").expect("failed to compile ui/app_window.slint");
}
