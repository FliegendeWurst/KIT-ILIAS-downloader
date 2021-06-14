# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

## [0.2.23] - 2021-06-14
### Added
- Logging output of saved forum post attachments

### Changed
- ILIAS folder/course pages are now always saved

### Fixed
- Links in saved ILIAS pages now work (see [`<base>`](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/base))
- ZIP of multiple forum post attachments is no longer downloaded

## [0.2.22] - 2021-06-02
### Added
- `--sync-url` can now download more "personal desktop" pages
- `--keep-session` flag to save and restore session cookies

## [0.2.21] - 2021-05-18
### Fixed
- Automatic output directory creation
- HTTP/2 [`NO_ERROR`](https://docs.rs/h2/0.3.3/h2/struct.Reason.html#associatedconstant.NO_ERROR) handling (issue [#15])
- Correct logging output when the progress bar is displayed

## [0.2.20] - 2021-05-13
### Fixed
- Status display no longer prints every path when running in a small terminal

## [0.2.19] - 2021-05-11
### Fixed
- Status display on Windows (issue [#14])

## [0.2.18] - 2021-05-07
### Added
- Request rate limiting (default 8 req. / 60 s, option `--rate`, issue [#13])

## [0.2.17] - 2021-05-04
### Added
- Progress/status display: `[15/40+] <path currently processed>`
- Extraction of course/folder pages (in course.html / folder.html, currently not versioned)

### Fixed
- Downloading of external images in forum posts
- Miscellaneous internal improvements

## [0.2.16] - 2021-04-19
### Added
- `--sync-url` option (to download only a single course/folder)
- `--user` and `--password` options (issue [#10])
- `--keyring` option (to get/save the password using a system keyring service)
- Colored errors/warnings (PR [#11] by [@thelukasprobst])

## [0.2.15] - 2021-04-14
### Added
- Downloading of attachments and embedded images in forum posts
- SOCKS5 proxy support (PR [#9] by [@Craeckie])

## [0.2.14] - 2021-02-16
### Fixed
- Handling of long paths on Windows (issue [#6])
- OpenCast downloading (issue [#7], PR [#8] by [@funnym0nk3y])

## [0.2.13] - 2021-01-05
### Fixed
- Shibboleth login (issue [#5], PR [#4] by [@Ma27])

## [0.2.12] - 2020-12-10
### Fixed
- Handling of invalid filenames on Windows (issue [#3])

## [0.2.11] - 2020-12-04
### Fixed
- Waiting on spawned tasks (issue [#2])

## [0.2.10] - 2020-11-27
### Added
- `.iliaslogin` file to provide login credentials

### Fixed
- Handling of `/` and `\\` in lecture names

## [0.2.9] - 2020-11-01
### Fixed
- OpenCast downloading

## [0.2.8] - 2020-07-16
### Fixed
- OpenCast downloading

## [0.2.7] - 2020-07-15
### Added
- Automatic creation of output directory
- Optional re-check of OpenCast lectures (`--check-videos`)

### Fixed
- OpenCast pagination (20 -> 800)

## [0.2.6] - 2020-05-18
### Added
- Downloading of exercise solutions and feedback

### Fixed
- Video filenames no longer contain raw HTML

## [0.2.5] - 2020-05-09
(undocumented)

## [0.2.4] - 2020-04-28
(undocumented)

## [0.2.3] - 2020-04-24
(undocumented)

## [0.2.2] - 2020-04-22
(undocumented)

## [0.2.1] - 2020-04-22
(undocumented)

## [0.2.0] - 2020-04-22
(undocumented)

## [0.1.0] - 2020-04-21
(undocumented)

[#15]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/15
[#14]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/14
[#13]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/13
[#11]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/pull/11
[#10]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/10
[#9]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/pull/9
[#8]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/pull/8
[#7]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/7
[#6]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/6
[#5]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/5
[#4]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/pull/4
[#3]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/3
[#2]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/2
[@thelukasprobst]: https://github.com/thelukasprobst
[@Craeckie]: https://github.com/Craeckie
[@funnym0nk3y]: https://github.com/funnym0nk3y
[@Ma27]: https://github.com/Ma27
[Unreleased]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.23...HEAD
[0.2.22]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.22...v0.2.23
[0.2.22]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.21...v0.2.22
[0.2.21]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.20...v0.2.21
[0.2.20]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.19...v0.2.20
[0.2.19]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.18...v0.2.19
[0.2.18]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.17...v0.2.18
[0.2.17]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.16...v0.2.17
[0.2.16]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.15...v0.2.16
[0.2.15]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.14...v0.2.15
[0.2.14]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.13...v0.2.14
[0.2.13]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.12...v0.2.13
[0.2.12]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.11...v0.2.12
[0.2.11]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.10...v0.2.11
[0.2.10]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.9...v0.2.10
[0.2.9]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/1529a678e0f4c78a24c8cc2cd236ce12ea8a78dc..v0.1.0
