# Tips for macOS users

## Git history viewer

There are various viewers for the Git history. "git log" on the
command line is one, "git log -p" with the changes, "git log --graph
--all --decorate -p" for yet more information. IDEs like vscode have
their own. A graphical one that you can use independent of any IDE,
with good funtionality, is "gitk", which you can install via:

    brew install git-gui

Run `man gitk` for the options it takes. Run e.g. `gitk --all &`.

## Configure editor

Both Git and the xmlhub tool rely on the `EDITOR` environment variable. XXX how to change, how to use e.g. vscode.

## SSH public key logins

ssh-agent XXX

