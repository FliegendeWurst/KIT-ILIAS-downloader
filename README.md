# KIT-ILIAS-downloader

Download content from ILIAS. That includes:

* files
* exercise sheets and solutions
* Opencast lectures
* forum posts

## Installation

Go to the [releases](../../releases) and get the executable for your operating system (Windows/Linux only).  
Or compile from source (mandatory if you use a Mac):
```
$ git clone https://github.com/FliegendeWurst/KIT-ILIAS-downloader
$ cd KIT-ILIAS-downloader
$ cargo install --all-features --path .
```

## Usage

First, open a terminal. Navigate to the directory that contains the downloaded binary.

Use `-o <directory>` to specify the download directory:

```
$ KIT-ILIAS-downloader -o ./ILIAS
```

By default, only content on your [personal desktop](https://ilias.studium.kit.edu/ilias.php?baseClass=ilPersonalDesktopGUI&cmd=jumpToSelectedItems) will be downloaded.  
Use the `--sync-url` option to download a specific page and its sub-pages: (the URL should be copied from an ILIAS link, not the browser URL bar)

```
$ KIT-ILIAS-downloader -o ./ILIAS/WS2021-HM1 --sync-url 'https://ilias.studium.kit.edu/ilias.php?ref_id=1276968&cmdClass=ilrepositorygui&cmdNode=uk&baseClass=ilRepositoryGUI'
```

### Options

```
$ KIT-ILIAS-downloader --help
KIT-ILIAS-downloader 0.2.18

USAGE:
    KIT-ILIAS-downloader [FLAGS] [OPTIONS] --output <output>

FLAGS:
        --check-videos    Re-check OpenCast lectures (slow)
        --content-tree    Use content tree (experimental)
    -f                    Re-download already present files
    -t, --forum           Download forum content
    -h, --help            Prints help information
        --keyring         Use the system keyring
    -n, --no-videos       Do not download Opencast videos
    -s, --skip-files      Do not download files
    -V, --version         Prints version information
    -v                    Verbose logging

OPTIONS:
    -j, --jobs <jobs>            Parallel download jobs [default: 1]
    -o, --output <output>        Output directory
    -P, --password <password>    KIT account password
    -p, --proxy <proxy>          Proxy, e.g. socks5h://127.0.0.1:1080
        --rate <rate>            Requests per minute [default: 8]
        --sync-url <sync-url>    ILIAS page to download
    -U, --username <username>    KIT account username
```

### .iliasignore

.gitignore syntax can be used in a `.iliasignore` file: (located in the download folder)
```ignore
# example 1: only download a single course
/*/
!/InsertCourseHere/
# example 2: only download files related to one tutorial
/Course/Tutorien/*/
!/Course/Tutorien/Tut* 3/
```

### Credentials

You can use the `--user` and `--keyring` options to get/store the password using the system password store.  
If you use Linux, you'll have to compile from source to be able to use this option.
```
$ KIT-ILIAS-downloader -U uabcd --keyring [...]
```

You can also save your username and password in a `.iliaslogin` file: (located in the output folder)
```
username
password
```

## Similar programs

- https://github.com/brantsch/kit-ilias-fuse/
- https://github.com/Garmelon/PFERD/
- https://github.com/DeOldSax/iliasDownloaderTool
