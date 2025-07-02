# Contributing to XML Hub with the `xmlhub` tool

The XML Hub is a Git repository with [BEAST2](https://www.beast2.org/)
XML files that could be useful to learn techniques from.

The `xmlhub` tool has two purposes:

1. It builds an index of the files to make it easier to find files when you're looking for a solution for some problem.

1. It helps contributing your own XML files to the repository.

It is currently a command line tool meant to download and use locally.
While you can work without it--either with just Git and
a text editor or even by using the GitLab web user interface--using
the tool will make you more efficient. Here are the steps to
follow to contribute using it.

Note that all `xmlhub` subcommands also have help directly in the
terminal: run `xmlhub --help` (or `xmlhub help`) for an overview over
all subcommands, or `xmlhub <subcommand> --help` (or `xmlhub help
<subcommand>`) for the help on a particular subcommand.

## Making sure you have the tool

1. You should install the `xmlhub` tool if you haven't already, so
   that you can run it without having to give the path: `xmlhub
   install`.  (If you want to tell someone else how to download the
   tool the first time: `git clone
   git@cevo-git.ethz.ch:cevo-resources/xmlhub-indexer-binaries.git`,
   then `cd xmlhub-indexer-binaries/macOS/x86_64/`, then `./xmlhub
   install`.  If you would like more assurance that you're installing
   our binary, you can [verify our PGP signatures](signatures.html).)

2. You should run a recent version. Run `xmlhub upgrade` to have it
   fetch and install the newest version. (This is secure, the upgrade
   will stop with an error if the downloaded binary does not have a
   valid signature from us XML Hub maintainers.) The tool *will*
   require you to run the upgrade when it detects that other people
   have used a version that produces different output than yours; so
   if you're feeling lazy, feel free to delay the upgrade until you
   have to.

## Making your contribution

Usually, you will follow these steps. Note that each of the
subcommands used here has their own `--help`, too! There are options
to adapt them in case you need to do things a bit differently.

1. If you don't already have a clone of the
   {xmlhubRepoLink}
   repository, run `xmlhub clone-to <path-to-a-directory>`, where
   `<path-to-a-directory>` is either an existing directory into which
   to make a `xmlhub` subdirectory, or a path including the name of
   the (not yet existing) directory that should be the clone.
   
   If you already do have a clone, instead run `cd
   <path-to-xmlhub-clone>` then `git pull` to update it. Having it up
   to date before starting your changes avoids potential merge
   conflicts when you want to push those.
   
   If you're new, and you would like to first experiment with
   contributing to XML Hub, you can instead run `xmlhub clone-to
   <path-to-a-directory> --experiments`. This clones the
   {xmlhubExperimentsRepoLink}
   repository instead that is not used for the real exchange of files.

1. Create a subdirectory for the project if there is none yet: `mkdir <Your subdirectory name>`, `cd <Your subdirectory name>`. (You can use the tabulator key to have the shell complete a path for you, especially if it contains spaces or other special characters!)

1. To copy an XML file: `xmlhub add-to <path-to-directory> <path-to-your-file.xml>`. If you ran the `cd <Your subdirectory name>` above, then `<path-to-directory>` is just `.`. Note that the `add-to` does remove sequences data by default. Read the terminal output to see if it did and what option to use to keep it.

    Alternatively, you can copy XML files into the xmlhub checkout via the macOS finder, then afterwards run: `xmlhub prepare <path-to-your-file.xml>`.

1. Edit the copied / prepared XML file(s) with your editor of choice. `open <path-to-your-file.xml>` might open the file in the right editor, otherwise find your xmlhub clone from the editor's user interface.

    To learn what you should enter for the various attributes, and which are optional, run `xmlhub help-attributes` or click the "Attributes list" item in the site navigation above or [click here](attributes.html).

1. Run `xmlhub check --open <path-to-your-file(s).xml>`. If this shows errors, you need to fix the problems. If on the other hand it opens the web brower, you can verify that the generated index lists your file(s) the way you wanted. You can run this command repeatedly, until you are satisfied with your edits.

1. Once you're done preparing your files, run `git add <path-to-your-file(s).xml>`, or `git add .` when inside the folder with your files, then `git commit -m "my commit message"`. Change "my commit message" to be somewhat descriptive. You can also run `git commit`, that opens the editor set in the `EDITOR` environment variable, on macOS, that is by default vim. If you don't know how to use this editor, just type `:`, `q`, then the return key, to get out of it.

1. To conclude your contribution, run `xmlhub build`, which updates the index to the latest files and verifies that you didn't forget to add or commit any files. In case it shows errors about files that are not yours, add the `--write-errors` option to force it to accept the state anyway. Then run `git push` to push your changes to GitLab. Congrats, now your changes should be visible from the GitLab web user interface at {xmlhubRepoLink}, too. Thanks!

For more information on contributing, see [CONTRIBUTE](https://cevo-git.ethz.ch/cevo-resources/xmlhub/-/blob/master/CONTRIBUTE.md) -- NOTE: currently this page is partially outdated! (XX TODO)

Don't hesitate to contact your XML Hub maintainer if you have any questions or suggestions!
