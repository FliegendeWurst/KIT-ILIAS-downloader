# KIT-ILIAS-downloader

Download content from ILIAS. That includes:

* files (latest version)
* Opencast lectures

## Installation

Go to the [releases](../../releases/latest) and get the executable for your operating system. Alternatively compile from source:
```sh
$ git clone https://github.com/FliegendeWurst/KIT-ILIAS-downloader
...
$ cd KIT-ILIAS-downloader
$ cargo build --release
...
$ cp target/release/KIT-ILIAS-downloader [directory in $PATH]
```

## Usage

Use `-o ILIAS` to set the download directory and `-j 5` to speed up the download. Username and password have to be provided every time the program is run.

```sh
$ KIT-ILIAS-downloader --help
KIT-ILIAS-downloader 0.2.0

USAGE:
    KIT-ILIAS-downloader [FLAGS] [OPTIONS] --output <output>

FLAGS:
    -f                  Re-download already present files
    -t, --forum         Download forum content
    -h, --help          Prints help information
    -n, --no-videos     Do not download Opencast videos
    -s, --skip-files    Do not download files
    -V, --version       Prints version information
    -v                  Verbose logging (print objects downloaded)

OPTIONS:
    -j, --jobs <jobs>        Parallel download jobs [default: 1]
    -o, --output <output>    Output directory
```

## Credits

Inspired by https://github.com/brantsch/kit-ilias-fuse. Re-implemented because the FUSE downloads were too slow and truncated.
