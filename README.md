# more-effective-intrans

—— 一个轻量的拼音转英文翻译器。

---

## 想法来源

使用NVim过程中，避免频繁切换中英文输入法，通过输入蹩脚的英文，来保证跟Agent对话大意的正确性。

---

## 环境

> 开发环境

| 项目 | 内容 |
| :--- | :--- |
| 系统 | Ubuntu 24.04.4 LTS |
| 工具链 | Rust 1.97.1（cargo 同版本，`edition = "2024"`） |
| 词库 | rime/rime-pinyin-simp 的 `pinyin_simp.dict.yaml`（Apache-2.0）；出处与校验值见 `assets/pinyin-simp/README.md` |
| 打包 | cargo-deb 3.8.0 |
| 依赖版本 | 见 `Cargo.toml`：Slint 1.18、Tokio 1.53、reqwest 0.13、global-hotkey 0.8、enigo 0.6、x11rb 0.14 |

> 运行环境

- Linux + **X11（Xorg）会话**。Wayland 下不可用：全局热键库在 Linux 上只实现了 X11。
- 验证过的桌面：Ubuntu 24.04 + GNOME Shell 46（Xorg）。
- 托盘图标依赖系统的 SNI 宿主，GNOME 下由 `ubuntu-appindicators@ubuntu.com` 扩展提供（Ubuntu 默认自带）。

---

## 使用

程序常驻托盘，没有主窗口。

1. 按 `Alt + ;` 唤出面板；
2. 在面板里敲拼音（例如 `pingguo`）回车，词库给出候选；
3. 按 `1` / `2` / `3` 或直接点击选一个词；
4. 译文直接落进"按热键那一刻焦点所在的窗口"里；

---

## 许可

本项目采用 GPL-3.0-only 许可，全文见 [`LICENSE`](LICENSE)。

---