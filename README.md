# KIT-ILIAS-downloader

Download content from ILIAS. That includes:

* files
* exercise sheets and solutions
* Opencast lectures
* forum posts

## Installation

**Windows/Linux users**: go to the [releases](../../releases) and download the executable for your operating system.   
**macOS users**: [Install Rust](https://www.rust-lang.org/tools/install) and compile from source:
```
$ cargo install --all-features --git 'https://github.com/FliegendeWurst/KIT-ILIAS-downloader'
```

## Usage

First, open a terminal. Navigate to the directory that contains the downloaded binary.

Then execute the program (use `-o <directory>` to specify the download directory):

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
KIT-ILIAS-downloader 0.3.3

USAGE:
    KIT-ILIAS-downloader [FLAGS] [OPTIONS] --output <output>

FLAGS:
        --all                 Download all courses
        --check-videos        Re-check OpenCast lectures (slow)
        --combine-videos      Combine videos if there is more than one stream (requires ffmpeg)
        --content-tree        Use content tree (experimental)
    -f                        Re-download already present files
    -t, --forum               Download forum content
    -h, --help                Prints help information
        --keep-session        Attempt to re-use session cookies
        --keyring             Use the system keyring
    -n, --no-videos           Do not download Opencast videos
        --save-ilias-pages    Save overview pages of ILIAS courses and folders
    -s, --skip-files          Do not download files
    -V, --version             Prints version information
    -v                        Verbose logging

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

.gitignore syntax can be used in a `.iliasignore` file: (located in the output folder)
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

### Renaming course names (0.2.24+)
If you'd like to avoid unwieldy course names (e.g. "24030 – Programmierparadigmen"), you can create a `course_names.toml` file in the output directory. It should contain the desired mapping of course names to folder names, e.g.:
```
"24030 – Programmierparadigmen" = "ProPa"
"Numerische Mathematik  für die Fachrichtungen Informatik und Ingenieurwesen" = "Numerik"
```

## Similar programs

- https://github.com/brantsch/kit-ilias-fuse/
- https://github.com/Garmelon/PFERD/
- https://github.com/DeOldSax/iliasDownloaderTool
