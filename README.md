# KIT-ILIAS-downloader

Download content from ILIAS. That includes:

* files (latest version)
* Opencast lectures

## Installation

Go to the [releases](../../releases) and get the executable for your operating system. Alternatively compile from source: (to get the latest updates)
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
You can put a `.iliasignore` in the output directory to skip some courses/folders/files.

```sh
$ KIT-ILIAS-downloader --help
KIT-ILIAS-downloader 0.2.3

USAGE:
    KIT-ILIAS-downloader [FLAGS] [OPTIONS] --output <output>

FLAGS:
        --content-tree    Use content tree (slow but thorough)
    -f                    Re-download already present files
    -t, --forum           Download forum content
    -h, --help            Prints help information
    -n, --no-videos       Do not download Opencast videos
    -s, --skip-files      Do not download files
    -V, --version         Prints version information
    -v                    Verbose logging (print objects downloaded)

OPTIONS:
    -j, --jobs <jobs>        Parallel download jobs [default: 1]
    -o, --output <output>    Output directory
```

## Related programs

- https://github.com/brantsch/kit-ilias-fuse (synchronous networking and sometimes (?) truncated downloads)
- https://github.com/Garmelon/PFERD/ (currently in the middle of a rewrite)
