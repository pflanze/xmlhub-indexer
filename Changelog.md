# Changelog

Newest entries at the bottom. Releases include the changes listed *above* them.

Versions starting with `v` are releases with binaries produced, whereas versions starting with `cj` are just done by Christian as way to sign a Git commit at convenient points (others are free to follow with their own initials). `xmlhub changelog` removes those latter tags.

- Initial development

v1 - 2025-01-07

- Fix Cargo.toml to refer to published versions

v1.1 - 2025-01-07

- Internally, sort paths from `git ls-files` for worry of different sort order on macOS (turned out unnecessary, the difference on macOS was due to erroneously using the wrong program version)

v1.2 - 2025-01-07

- Internal improvements
- Remove benchmarking instrumentation again (concludes benchmarking work)

v1.3 - 2025-01-08

- When creating a signature with git fails, mention that `--local-user` should be used.
- Fix up `--local-user` arguments for macOS: replace non-breaking spaces copied from GUI tool with spaces for compatibility with command line gpg tool.

v1.4 - 2025-01-09

- Fix README on how to export a key with gpg

v1.5 - 2025-01-09

- README and docs improvements
- Remove `--path`, `--md`, `--html` options; `BASE_PATH` is now required
- Write an `attributes.md` file
- Add `--verbose` option for tracing calls to external commands
- Add `--batch` option
- Fix error message when other than version errors happen

v2 - 2025-01-13

- Replace `xmltree` + `xml-rs` crates with `roxmltree` for a large speedup. Remove `--no-wellformedness-check` option (now unused).
- `make-xmlhub-indexer-release`: check Cargo.toml for `path =` entries
- `make-xmlhub-indexer-release`: ignore commit check with `--dry-run`, too
- Docs and code improvements

v3 - 2025-01-17

- make-xmlhub-indexer-release improvements
- When building, ignore Git tags not starting with `v`
- Fix: allow `--version` to be run without having to give a `BASE_PATH`

v4 - 2025-01-19

- Add `--no-branch-check` option
- Change to jump to the box via the main link, instead use document icon to open the file
- Move the document symbol (link to XML file) *after* each path
- Show the document symbol after the link in file info boxes
- Various internal refactors
- In info boxes, for all indexed values add a link to their index entry
- Fix: pull from the remote *before* reading the paths
- Add `--daemon` mode

v5 - 2025-01-28

- carry location of comments from XML files into error messages
- on XML reading errors, show "lines" or "line" depending on plurality
- Use an SVG in place of the unicode document symbol due to the latter not showing on macOS

v6 - 2025-02-07

- for daemon mode, add --quiet option
- disable verbose messages in daemon mode
- various README edits, update --help text
- Add background daemon starting and stopping

v6.1 - 2025-02-24

- Add script in `examples/`
- daemon fix/improvement: continue logging until daemon closed the logging pipe, then say "daemon ended"
- `make-xmlhub-indexer-release`: make `--local-user` required by default with `--sign`
- `make-xmlhub-indexer-release`: make signing the default
- `make-xmlhub-indexer-release`: make pushing the default

v6.2 - 2025-03-03

- Rename `xmlhub-build-index` program to `xmlhub`
- add `build` command
- change to read untracked files by default (instead of asking Git for the file list, ask the file system), add `--ignore-untracked` for the old behaviour
- improve messaging, incl. be clearer about what the commit refusal means.
- add `--help-contributing`

cj9

- fix: verify the current branch when getting the remote
- Internal refactor: change git repository verification to typed verification steps, check early.
- docs improvements
- README updates
- use proper subcommands (via clap) with their own options
- improve error messages; nicer display of untracked files in error messages about refusing to commit.
- Move the state folder out of `.git/`, to `.xmlhub/` at the root of the working directory. Ignore that folder when getting the file list (necessary in transition period until entry is in `.gitignore`).
- add `clone-to` subcommand
- format `--help` output to fit the terminal width (finally).
- add `add` subcommand, blinds data and adds comment template.

cj10

- `add`: avoid overwriting target paths, add `--force` to do so
- add `prepare` subcommand, and fix docs on `add` subcommand
- `prepare`: make use of the modified status (don't overwrite file if unchanged)
- `prepare`: only add comment if modified
- add and use `trash` crate to remove files before writing to their place.

cj11

- change default comment for blinded data as discussed
- parse BEAST version number from XML files

cj12

- fix: only refuse version != 2 files *for blinding*

cj13

- change `--help-contributing` option into a `help-contributing` subcommand
- add `help-attributes` subcommand.
- add `desc` field (title "Description") to `AttributeSpecification`, add it to `attributes.md` and `help-attributes` output, add values for most attributes.

cj14

- `add/prepare`: say when data in a file was blinded (treat whitespace-only data as no data).
- `build`: make `BASE_PATH` optional: remove the positional argument and instead add a `--base-path` option.
- rename `add` command to `add-to`
- `add-to`: check that target_directory is in an xmlhub clone (verify correct repository by checking some subpaths)
- add `--no-repo-check` options, make `--batch` (and indirectly `--daemon`) imply them.
- Some internal refactoring for less error-prone option processing logic.
- prepare/add-to: add empty lines before and after comment template
- prepare/add-to: restrict to BEAST2 by default, add `--ignore-version`
- rename BEAST "major" to "product" version number
- daemon mode: periodically log activity even in `--quiet` mode
- ignore untracked files when committing in batch/daemon mode
- `prepare/add-to`: only add header comments if missing
- Cargo: add authors and license fields
- Improve english pluralization.
- `attributes.md`: make table titles bold.
- `build`, when writing errors to the index files: add title (mouse-over) on file paths, and show the document symbol on file paths.
- `build`: remove nonsensible `--timestamp` feature
- Change build optimization to aim for smaller size.
- `clone`: check program version against the repository after cloning.
- `add-to`: verify program version against the repo being added to.
- `add-to`: nicer message--don't pretend to do something with no files.
- Improvements of terminal messages.

cj15

- Improvements of terminal messages.
- `add-to` and `prepare` now mention when data has been removed (and hint at the `--no-blind` option)
- `add-to` now advises to use `help-attributes` after finishing; it also shows the target file path(s) so that those can easily be seen or copy pasted.

cj16

- Add a `check` subcommand, which allows to explicitly check one's new file, ignoring other errors from other files/people, and without committing.

v7 - 2025-04-03

- `DOI`: add description, do not autolink, change into a list
- Add `Repository` attribute, 

v7.1 - 2025-04-03

- Fix make-xmlhub-indexer-release: allow `path` in `Cargo.toml`, as long as they are into the local repository.
- Add installation infrastructure (not used yet) that installs into `~/.cargo/bin/` even if cargo is not installed, and adds code to shell startup files to add that to the `PATH`.
- Add signing infrastructure based on fips205 and a custom JSON based file format (includes a reusable abstraction for JSON file based type serialisation).
- Add creation/reading of application `.info` files (`AppInfo` type).
- Add internal sha2 hashing so that the binary does not need to rely on an external `sha256sum` command (also adds a `sha256sum-rs` binary, only meant for testing).
- `make-xmlhub-indexer-release`: in data collection phase, add a check that push will not fail.

cj17

- `make-xmlhub-indexer-release`: create app info files, and sign them.
- JSON files now come in two modes: overwritable (e.g. `.info` files) and exclusive (do not overwrite, e.g. key files).

cj18

- Add `xmlhub install` subcommand.
- Fixes for shell handling and code to set up `PATH` in installation process.
- Add `zsh` support.

v7.2 - 2025-04-08

- Fix `xmlhub check`: handle relative paths from the current directory.
- `xmlhub --version`: show architecture, and compilation profile.
- Change README to recommend `cargo run --bin xmlhub --release install` instead of `cargo install --path .` (the latter ignores `Cargo.lock` by default which is undesirable for security reasons, also don't want to install the binaries other than `xmlhub`).
- Add `xmlhub upgrade`

cj19

- `xmlhub clone-to`: add `--experiment` option.

v7.3 - 2025-04-09

- `xmlhub clone-to`: treat the path argument like `cp` does

v7.4 - 2025-04-10

- Link `DOI` entries
- Autolink values in index key positions, too.
- From file info boxes, link back from individual values (when indexed) to the index via *separate* links (using up-arrow symbols) to allow automatic links to be used, too.

v8 - 2025-04-11

- `xmlhub build --daemon`: set resource limits in worker child processes (obviating the need for `ulimit` in shell wrapper files, and avoiding the problem of long-running daemons probably being killed due to CPU limit).
- `xmlhub build --daemon`: set CPU priority in worker child processes to 10 (be nicer to other users on the server).
- Add `xmlhub --version-only` option; for `--version`, also show the OS.
- Add `Changelog.md`, and `xmlhub changelog` subcommand.
- `xmlhub help-attributes`: add `--open`, for the nicer HTML view, and make it the default, add `--print` for old behaviour.
- `xmlhub clone-to`: rename `--experiment` option to `--experiments`
- `xmlhub build`: make it work when the current working directory is in a *subdirectory* of the repo
- Rename `make-xmlhub-indexer-release` to `make-release` (now that we don't install it anymore, it's fine to use a generic name)
- When a version check on a repo detects an outdated executable, instruct the user to run `xmlhub upgrade`
- `xmlhub upgrade`: verify that the remote version is actually newer, unless --force-* options are given
- `make-release`: remind the user to update the `Changelog.md` file
- `make-release`: update `Changelog.md` (after checking first) with release tag and date

v8.1 - 2025-04-13

- `xmlhub help-attributes`: actually use the larger Markdown variant with additional text (reformat as HTML) for the browser view
- attributes: add suggestion to talk to the maintainer with ideas
- `make-release`: build both intel and ARM binaries on the mac
- fix `make-release`: when adding a release to the Changelog, in messaging including .info files refer to the commitid *after* that happened

v9 - 2025-04-15

- Add `xmlhub docs` subcommand, and designate it as the primary entry point for all new users, or when forgetting how things work. This also reaches the page with the attributes list, and all existing and  future pages share a navigation bar.
- Remove the hints about how to set the browser, to reduce clutter in the `--help` view--it appears the browser setup works fine on macOS, thus should be fine. Should add this info somewhere else in the future.

v9.1 - 2025-04-17

- `make-release`: show a shorter, more human-readable, description of what it will do.
- `xmlhub install`: add `--confirm` option, so you can review what the install action is doing before it is being carried out. Also show more detail (both with and without `--confirm`).
- `xmlhub upgrade`: add `--confirm` option similar to the one for `xmlhub install`.
- `make-release`/`xmlhub upgrade`: copy the changelog into the xmlhub-indexer-binaries repository, and show it when upgrading.

v9.2 - 2025-04-21

- Fix `xmlhub clone-to --experiments` to actually clone the xmlhub-experiments repository again.
- docs: mention the `--experiments` option.
- `make-release`: add cross compilation from macOS to Linux (with musl)

v9.3 - 2025-04-22

- fix scanning for version messages in git log output: handle Merge commits

v9.4 - 2025-04-22

- `xmlhub install`: do not try to install if this is the installed binary (do not delete the target and then fail copying the source)
- `xmlhub install/upgrade`: better message, say when nothing was done, and mention when a new shell is required
- `xmlhub upgrade`: do a shallow clone (--depth 1) of the binaries repository to reduce download size
- `xmlhub install/upgrade`: log information about the installations in `~/.xmlhub/upgrades-log/`
- `xmlhub`: move all options to the individual subcommands: the only global options are now `--version` and `--version-only` (and the help options)
- `xmlhub build --daemon`: add hack to allow running with the `HOME` env var set to another dir than the user's home, adding symlink for `.ssh` to the original home if nothing is there, to allow to use ssh while the home dir mount is gone and the bare underlying filesystem shows through.

v9.5 - 2025-04-25

- `xmlhub build --daemon`: fix hack for Linux musl binaries

v9.6 - 2025-04-25

- fix the fix of the fix: the problem was *not* musl, but stadler09
- `xmlhub build --daemon`: remove hack again, use HTTPS and gitlab tokens instead

v9.7 - 2025-04-25

- `xmlhub changelog`: open in the browser by default, add `--print` option (mirror the behaviour of `xmlhub help-attributes`; drop the `--print-html` option).
- `xmlhub changelog`: simplify changelog title
- `xmlhub build --daemon run|start`: get working on macOS again (do not try to set up a memory limit on macOS, macOS does not support it); add `--limit-as` option at the same time (for Linux only)
- `xmlhub help-attributes` / in `attributes.md`: hint at the xmlhub command line tool
- Index "Citations" and make the field into a "|"-separated list
- Edit descriptions of "Citation" and "DOI"
- Swap the order of the "DOI" and "Citation" fields, prefer "DOI" only (in view of near-future automatic retrieval of the paper details from the DOI)
- Change the `desc` metainfo field to accept Markdown (allowing for formatting and links in attribute descriptions)
- Add workaround to make the `-h` and `--help` options behave the same way
- xmlhub help: update the 'blurb' about the program (about text shown first in the help)
- xmlhub build error messages: list missing keywords sorted in the order of their definition (same as shown in templates or the attributes list)
- Some nicer error messages
- xmlhub build: add capability to collect warnings (for both terminal and index file output); check the BEAST version from the `<beast>` element, compare to the one passed manually, warn if different.
- `xmlhub prepare / add-to`: fix the blinding feature: retain the `<sequence>` elements but empty their values attribute; add `--blind-all` option to get the old behaviour, and mention in the message that the option exists and that the default blinding does not strip the metadata (thus one has to be careful not to have privacy sensitive information in the metadata!)
- `xmlhub prepare`: check that the file size is below 5 MB, provide an option to override
- for `xmlhub docs`: implement variables for repository urls and version information that documentation pages can use
- `xmlhub docs`: add a (preliminary) logo, link to the BEAST2 website, add link to XML Hub repository to the navigation bar, add description about XML Hub and the xmlhub tool, add a preliminary macOS user page, add an "about" page with version info, add "signatures verification" page
- Implement attributes which are derived from other attributes. Also adds attribute "Citation via DOI", but unfinished, currently just shows a copy of the DOI itself.
- Add extracted attributes, and "Contains sequence data" attribute (finished)

v10 - 2025-06-23

- Add DaemonMode::StartIfNotRunning for `xmlhub build --daemon start-if-not-running` to silently do nothing if already running (good for crontab)
- xmlhub: do *not* imply `--no-repo-check` with batch (daemon) mode (even daemon mode users make errors)
- xmlhub: *do* imply `--quiet` with batch/daemon mode

v10.1 - 2025-06-23

- Added new script examples/start-xmlhub-via-crontab
- `xmlhub docs` now supports handlebars templates for markdown files, with currently just one variable, a boolean `public`, which is true when the environment variable `XMLHUB_PUBLIC` is set to `1`.
- Add blinder Python scripts in case anyone prefers to use those over the same functionality in the `xmlhub` tool.
- Various documentation improvements in `xmlhub docs`.
