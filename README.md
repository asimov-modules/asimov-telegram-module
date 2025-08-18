# ASIMOV Telegram Module

[![License](https://img.shields.io/badge/license-Public%20Domain-blue.svg)](https://unlicense.org)
[![Compatibility](https://img.shields.io/badge/rust-1.85%2B-blue)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/)
[![Package](https://img.shields.io/crates/v/asimov-telegram-module)](https://crates.io/crates/asimov-telegram-module)

ASIMOV module for integration with the Telegram messaging service.

## 🛠️ Prerequisites

- [Rust](https://rust-lang.org) 1.88+ (2024 edition)
- [OpenSSL](https://www.openssl.org) 3.5+
- [zlib](https://www.zlib.net) 1.3+

## ⬇️ Installation

### Installation with the [ASIMOV CLI] (recommended)

```bash
asimov module install telegram -v
```

### Installation from Source Code

Make sure to follow the instructions from [development](#-development) section.

```bash
cargo install asimov-telegram-module
```

## 👉 Configuration

To start using the module you need to get authorized first:

```console
asimov module config telegram
# or directly:
asimov-telegram-configurator
```

## 👨‍💻 Development

While for pre-built binaries we provide our own Telegram application credentials,
for development purposes you will have to create your own Telegram application.
You can do this here: `https://my.telegram.org/`

Then you will need to fill next environment variables:

- `ASIMOV_TELEGRAM_API_ID`
- `ASIMOV_TELEGRAM_API_HASH`

Make sure that OpenSSL & zlib are installed on your system.

On Windows you can install them with [vcpkg](https://github.com/microsoft/vcpkg):

```bash
vcpkg install openssl:x64-windows-static-md
vcpkg install zlib:x64-windows-static-md
```

On Linux you also need to install the following packages:

```bash
sudo apt-get install -y libc++-dev libc++abi-dev
```

Then finally you can start development:

```bash
git clone https://github.com/asimov-modules/asimov-telegram-module.git
```

### Resources

- <https://core.telegram.org/tdlib/getting-started>
- <https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1_function.html>
- <https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1_object.html>
- <https://core.telegram.org/tdlib/docs/classes.html>

---

[![Share on X](https://img.shields.io/badge/share%20on-x-03A9F4?logo=x)](https://x.com/intent/post?url=https://github.com/asimov-modules/asimov-telegram-module&text=asimov-telegram-module)
[![Share on Reddit](https://img.shields.io/badge/share%20on-reddit-red?logo=reddit)](https://reddit.com/submit?url=https://github.com/asimov-modules/asimov-telegram-module&title=asimov-telegram-module)
[![Share on Hacker News](https://img.shields.io/badge/share%20on-hn-orange?logo=ycombinator)](https://news.ycombinator.com/submitlink?u=https://github.com/asimov-modules/asimov-telegram-module&t=asimov-telegram-module)
[![Share on Facebook](https://img.shields.io/badge/share%20on-fb-1976D2?logo=facebook)](https://www.facebook.com/sharer/sharer.php?u=https://github.com/asimov-modules/asimov-telegram-module)
[![Share on LinkedIn](https://img.shields.io/badge/share%20on-linkedin-3949AB?logo=linkedin)](https://www.linkedin.com/sharing/share-offsite/?url=https://github.com/asimov-modules/asimov-telegram-module)
