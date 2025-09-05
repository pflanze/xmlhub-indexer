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

Both Git and the xmlhub tool rely on the standard `EDITOR` environment
variable to choose the editor to run when needed. For editors that
wait by default until the edited file is closed, you can just set that
environment variable, which you can do in the `.zshenv` file in your
home directory: e.g. `export EDITOR=vim` on its own line.

For editors that immediately return and do not wait for the file to be
closed, but allow to wait by giving command line arguments, you can
make a wrapper script and set `EDITOR` to point to that
script. E.g. for VS code:

1. Create a directory `bin` in your home if you don't have it
   already. Create a file `code-wait` inside it, and add the following:
   
        #!/bin/bash
        exec code --wait "$@"

2. Add `export EDITOR=~/bin/code-wait` to `.zshenv` as mentioned above.

## SSH public key logins

For frequent access to the git repository via ssh, it is more convenient to setup the ssh-agent.

Have a look at [this tutorial](https://usercomp.com/news/1044072/using-ssh-agent-on-mac).

The following code is a bit shorter and also works.

```
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.user.ssh-agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/bin/ssh-agent</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
```
