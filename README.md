# KIT-ILIAS-downloader

Download content from ILIAS. That includes:

* files (latest version)
* Opencast lectures

## Installation

Go to the [releases](../../releases) and get the executable for your operating system. Alternatively compile from source: (to get the latest updates)
```
$ git clone https://github.com/FliegendeWurst/KIT-ILIAS-downloader
...
$ cd KIT-ILIAS-downloader
$ cargo build --release
...
$ cp target/release/KIT-ILIAS-downloader [directory in $PATH]
```

## Usage

Use `-o <directory>` to specify the download directory. Username and password have to be provided every time the program is run.

Only content on your [personal desktop](https://ilias.studium.kit.edu/ilias.php?baseClass=ilPersonalDesktopGUI&cmd=jumpToSelectedItems) will be downloaded.

```
$ KIT-ILIAS-downloader --help
KIT-ILIAS-downloader 0.2.5

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

### .iliasignore

.gitignore syntax can be used in a `.iliasignore` file: (located in the download folder)
```ignore
# example: only download a single course
/*/
!/InsertNameHere/
```

## Similar programs

- https://github.com/brantsch/kit-ilias-fuse/
- https://github.com/Garmelon/PFERD/
