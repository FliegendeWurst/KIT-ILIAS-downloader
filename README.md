# KIT-ILIAS-downloader

> [!Warning]
> The program no longer works since the upgrade to ILIAS 9: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/63
> 
> Meanwhile you can use the more complicated [PFERD](https://github.com/Garmelon/PFERD), or step up to maintain this downloader :)

Download content from ILIAS. That includes:

* files
* exercise sheets and solutions
* Opencast lectures
* forum posts

## Installation

**Windows/Linux users**: go to the [releases](../../releases) and download the executable for your operating system.   

**macOS users**: 
[Install Rust](https://www.rust-lang.org/tools/install):
```
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
and compile from source:
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
KIT-ILIAS-downloader 0.3.5

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
    -j, --jobs <jobs>              Parallel download jobs [default: 1]
    -o, --output <output>          Output directory
        --pass-path <pass-path>    Path inside `pass(1)` to the password for your KIT account
    -P, --password <password>      KIT account password
    -p, --proxy <proxy>            Proxy, e.g. socks5h://127.0.0.1:1080
        --rate <rate>              Requests per minute [default: 8]
        --sync-url <sync-url>      ILIAS page to download
    -U, --username <username>      KIT account username
```

### .iliasignore

.gitignore syntax can be used in a `.iliasignore` file: (located in the output directory)
```ignore
# example 1: only download a single course
/*/
!/InsertCourseHere/
# example 2: only download files related to one tutorial
/Course/Tutorien/*/
!/Course/Tutorien/Tut* 3/
```

### Credentials

You can use the `--user` and `--keyring` options to get/store the password using the system password store:
```
$ KIT-ILIAS-downloader -U uabcd --keyring [...]
```

If you use [pass](https://www.passwordstore.org/), you can use the `--pass-path` option to specify which password store entry to use:
```
$ KIT-ILIAS-downloader -U uabcd --pass-path edu/kit/uskyk [...]
```

You can also save your username and password in a `.iliaslogin` file: (located in the output folder)
```
username
password
```

When running the downloader multiple times in a short period of time, you may want to use the `--keep-session` flag.
If specified, the downloader will save and restore session cookies (`.iliassession` file in the output directory).

### Renaming course names (0.2.24+)
If you'd like to avoid unwieldy course names (e.g. "24030 – Programmierparadigmen"), you can create a `course_names.toml` file in the output directory. It should contain the desired mapping of course names to folder names, e.g.:
```
"24030 – Programmierparadigmen" = "ProPa"
"Numerische Mathematik  für die Fachrichtungen Informatik und Ingenieurwesen" = "Numerik"
```

## Troubleshooting
### Error when using `--keyring` option
```
PlatformFailure(Zbus(MethodError("org.freedesktop.DBus.Error.ServiceUnknown", Some("The name org.freedesktop.secrets was not provided by any .service files"), Msg { type: Error, sender: "org.freedesktop.DBus", reply-serial: 2, body: Signature: [
        s (115),
] })))
```
If you get the above error when activating the `--keyring` option, you need to make sure that your system keyring service (KeePassXC, GNOME Keyring, ...) is running.

## Other useful programs

- https://github.com/Garmelon/PFERD
- https://github.com/DeOldSax/iliasDownloaderTool
- https://github.com/brantsch/kit-ilias-fuse
- https://github.com/Mr-Pine/IliasUploaderUtility (unlike the other tools, this one uploads files)
- https://github.com/I-Al-Istannen/ilias-tests (unlike the other tools, this one processes "tests")
