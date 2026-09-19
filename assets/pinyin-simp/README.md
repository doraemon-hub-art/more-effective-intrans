#/**
# * @file README.md
# * @author doraemon-hub-art (1660219734@qq.com)
# * @brief Provenance of the bundled pinyin dictionary
# * @date 2026-09-19
# *
# * @copyright Copyright (c) 2026
# */

# 内置拼音词库出处

本目录下的 `pinyin_simp.dict.yaml` 是第三方数据，随程序一起分发，不在运行期单独安装。

| 项目 | 内容 |
| :--- | :--- |
| 来源 | https://github.com/rime/rime-pinyin-simp |
| 上游提交 | `0c6861ef7420ee780270ca6d993d18d4101049d0`（2024-10-05） |
| 文件 | `pinyin_simp.dict.yaml`，1,266,216 字节 |
| 校验值 | sha256 `e341598343a0f0f2035bb1aafc34a7f3bb7887deeecb3f60796262aaa2983e6b` |
| 拉取日期 | 2026-09-19 |
| 许可 | Apache-2.0（见同目录 `LICENSE`） |
| 作者 | 见同目录 `AUTHORS`；该词库派生自 Android Pinyin IME 的词库数据 |
| 规模 | 65,125 条，去空格后 38,999 个拼音键 |

## 使用约定

- 解析与查表规则见 `docs/arch.md`「拼音词库（词语查表）」一节。
- 更新词库的做法：替换 `pinyin_simp.dict.yaml`，同步本文件的提交号、校验值与拉取日期。
- 本目录只在编译期被引用（`include_str!`），运行期不读取文件。
