# ASIMOV Telegram Module

[![License](https://img.shields.io/badge/license-Public%20Domain-blue.svg)](https://unlicense.org)
[![Compatibility](https://img.shields.io/badge/rust-1.85%2B-blue)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/)
[![Package](https://img.shields.io/crates/v/asimov-telegram-module)](https://crates.io/crates/asimov-telegram-module)

ASIMOV module for integration with the Telegram messaging service.

## 🛠️ Prerequisites

- [Rust](https://rust-lang.org) 1.85+ (2024 edition)

## ⬇️ Installation

### Installation from Source Code

```bash
cargo install asimov-telegram-module
```

## 👉 Examples

You need to create credentials on `https://my.telegram.org/` to be able to authorize this module to access your Telegram account. Register an app such as `ASIMOV Telegram Module` and once you have you credentials set these environment variables: `export API_ID="12345"`, `export API_HASH="12345"`. The credentials are private to your account and should not be shared with others.

```console
$ asimov-telegram-cataloger send-code "+1234567890"
$ asimov-telegram-cataloger verify-code "12345"
$ asimov-telegram-cataloger
```

## 👨‍💻 Development

```bash
git clone https://github.com/asimov-modules/asimov-telegram-module.git
```

### Resources

- https://core.telegram.org/tdlib/getting-started
- https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1_function.html
- https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1_object.html
- https://core.telegram.org/tdlib/docs/classes.html

---

[![Share on X](https://img.shields.io/badge/share%20on-x-03A9F4?logo=x)](https://x.com/intent/post?url=https://github.com/asimov-modules/asimov-telegram-module&text=asimov-telegram-module)
[![Share on Reddit](https://img.shields.io/badge/share%20on-reddit-red?logo=reddit)](https://reddit.com/submit?url=https://github.com/asimov-modules/asimov-telegram-module&title=asimov-telegram-module)
[![Share on Hacker News](https://img.shields.io/badge/share%20on-hn-orange?logo=ycombinator)](https://news.ycombinator.com/submitlink?u=https://github.com/asimov-modules/asimov-telegram-module&t=asimov-telegram-module)
[![Share on Facebook](https://img.shields.io/badge/share%20on-fb-1976D2?logo=facebook)](https://www.facebook.com/sharer/sharer.php?u=https://github.com/asimov-modules/asimov-telegram-module)
[![Share on LinkedIn](https://img.shields.io/badge/share%20on-linkedin-3949AB?logo=linkedin)](https://www.linkedin.com/sharing/share-offsite/?url=https://github.com/asimov-modules/asimov-telegram-module)
