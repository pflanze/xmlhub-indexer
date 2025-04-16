# Contributing to XML Hub with the `xmlhub` tool

While you can work without the `xmlhub` tool--either with just Git and
a text editor or even by using the GitLab web user interface--using
the tool will make you more efficient. Here are the steps to
follow. Note that all commands also have help directly in the
terminal, via `xmlhub --help` or `xmlhub <subcommand> --help`.

## Making sure you have the tool

1. You should install the `xmlhub` tool if you haven't already, so
   that you can run it without having to give the path: `xmlhub install`.
   (If you want to tell someone else how to download the tool the first time:
   `git clone git@cevo-git.ethz.ch:cevo-resources/xmlhub-indexer-binaries.git`,
   then `cd xmlhub-indexer-binaries/macOS/x86_64/`, then `./xmlhub install`.
   XX TODO: link to document on how to verify the PGP fingerprint)

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
   [xmlhub](https://cevo-git.ethz.ch/cevo-resources/xmlhub)
   repository, run `xmlhub clone-to <path-to-a-directory>`, where
   `<path-to-a-directory>` is either an existing directory into which
   to make a `xmlhub` subdirectory, or a path including the name of
   the (not yet existing) directory that should be the clone.
   
   If you already do have a clone, instead run `cd
   <path-to-xmlhub-clone>` then `git pull` to update it. Having it up
   to date before starting your changes avoids potential merge
   conflicts when you want to push those.

1. Create a subdirectory for the project if there is none yet: `mkdir <Your subdirectory name>`, `cd <Your subdirectory name>`. (You can use the tabulator key to have the shell complete a path for you, especially if it contains spaces or other special characters!)

1. To copy an XML file: `xmlhub add-to <path-to-directory> <path-to-your-file.xml>`. If you ran the `cd <Your subdirectory name>` above, then `<path-to-directory>` is just `.`. Note that the `add-to` does remove sequences data by default. Read the terminal output to see if it did and what option to use to keep it.

    Alternatively, you can copy XML files into the xmlhub checkout via the mac finder, then afterwards run: `xmlhub prepare <path-to-your-file.xml>`.

1. Edit the copied / prepared XML file(s) with your editor of choice. `open <path-to-your-file.xml>` might open the file in the right editor, otherwise find your xmlhub clone from the editor's user interface.

    To learn what you should enter for the various attributes, and which are optional, run `xmlhub help-attributes`.

1. Run `xmlhub check --open <path-to-your-file(s).xml>`. If this shows errors, you need to fix the problems. If on the other hand it opens the web brower, you can verify that the generated index lists your file(s) the way you wanted. You can run this command repeatedly, until you are satisfied with your edits.

1. Once you're done preparing your files, run `git add <path-to-your-file(s).xml>`, or `git add .` when inside the folder with your files, then `git commit -m "my commit message"`. Change "my commit message" to be somewhat descriptive. You can also run `git commit`, that opens the editor set in the `EDITOR` environment variable, on the mac, that is by default vim. If you don't know how to use this editor, just type `:`, `q`, then the return key, to get out of it.

1. To conclude your contribution, run `xmlhub build`, which updates the index to the latest files and verifies that you didn't forget to add or commit any files. In case it shows errors about files that are not yours, add the `--write-errors` option to force it to accept the state anyway. Then run `git push` to push your changes to GitLab. Congrats, now your changes should be visible from the GitLab web user interface at [https://cevo-git.ethz.ch/cevo-resources/xmlhub](https://cevo-git.ethz.ch/cevo-resources/xmlhub), too. Thanks!

For more information on contributing, see [CONTRIBUTE](https://cevo-git.ethz.ch/cevo-resources/xmlhub/-/blob/master/CONTRIBUTE.md) -- NOTE: currently this page is partially outdated! (XX TODO)

Don't hesitate to contact your XML Hub maintainer if you have any questions or suggestions!

<!--
For longer documentation, see ...XX TODO
e.g. set up ssh-agent
-->
