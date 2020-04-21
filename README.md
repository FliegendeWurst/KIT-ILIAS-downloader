# KIT-ILIAS-downloader

Download content from ILIAS. That includes:

* files (latest version)
* Opencast lectures

## Installation

Go to the [releases](../../releases) and get the executable for your operating system. Alternatively compile from source:
```sh
$ git clone https://github.com/FliegendeWurst/KIT-ILIAS-downloader
...
$ cd KIT-ILIAS-downloader
$ cargo build --release
...
$ cp target/release/KIT-ILIAS-downloader [directory in $PATH]
```

## Usage

```sh
$ KIT-ILIAS-downloader --help
KIT-ILIAS-downloader 0.1.0

USAGE:
    KIT-ILIAS-downloader [FLAGS] --output <output>

FLAGS:
    -f,                 Re-download already present files
    -h, --help          Prints help information
    -n, --no-videos     Do not download Opencast videos
    -s, --skip-files    Do not download files
    -V, --version       Prints version information
    -v                  Verbose logging (print objects downloaded)

OPTIONS:
    -o, --output <output>    Output directory
```

## Credits

Inspired by https://github.com/brantsch/kit-ilias-fuse.