# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.3.8]
### Fixed
- Video downloads work again ([#54])

## [0.3.7]
### Fixed
- session expiration is now recognized more accurately ([#44])

## [0.3.6]
### Fixed
- `--all` once again downloads all courses you're a member of

## [0.3.5]
### Added
- `--pass-path` option to get the password from [pass](https://www.passwordstore.org/) (PR [#33] by [@Ma27])

## [0.3.4]
### Added
- Display a warning if two or more courses/folders have the same name ([#31])

### Fixed
- `--keyring` option on Linux now tries to unlock password before using it (previously, this error would occur: `PlatformFailure(Zbus(MethodError("org.freedesktop.Secret.Error.IsLocked", None, Msg { type: Error, sender: ":1.34", reply-serial: 6 })))`)
 
## [0.3.3] - 2022-03-21
### Addded
- `--all` flag to download all courses ([#30])

## [0.3.2] - 2022-01-21
### Fixed
- Downloading of videos (PR [#28] by [@funnym0nk3y])

## [0.3.1] - 2022-01-07
### Fixed
- `--sync-url` can now be used to download the [course memberships](https://ilias.studium.kit.edu/ilias.php?cmdClass=ilmembershipoverviewgui&cmdNode=iy&baseClass=ilmembershipoverviewgui)

## [0.3.0] - 2022-01-06
### Fixed
- ILIAS 7 update ([#27])

## [0.2.24] - 2021-11-01
### Added
- `--combine-videos` option to merge multiple video streams of the same lecture
- `--save-ilias-pages` option to also save the ILIAS overview pages of courses/folders
- Configuration file to change course names (PR [#19] by [@thelukasprobst])

### Fixed
- Downloading of lectures that consist of multiple streams

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

[#54]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/54
[#44]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/44
[#33]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/pull/33
[#31]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/31
[#30]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/30
[#28]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/pull/28
[#27]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/issues/27
[#19]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/pull/19
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
[Unreleased]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.3.7...HEAD
[0.3.7]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.3.7...v0.3.6
[0.3.6]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.3.6...v0.3.5
[0.3.5]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.3.5...v0.3.4
[0.3.4]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.24...v0.3.0
[0.2.24]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.23...v0.2.24
[0.2.23]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.23...v0.2.24
[0.2.23]: https://github.com/FliegendeWurst/KIT-ILIAS-downloader/compare/v0.2.22...v0.2.23
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
