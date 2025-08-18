# Run Git easily

This offers a way to interact with the `git` tool from an existing Git
installation conveniently. I.e. this trades a dependency on a Git
library (which also might not behave identical to the git tool, or be
difficult to make behave the same?, and makes for a large binary) for
a dependency on the normal git tool (that also will behave as you know
it, but might have to be installed separately by the user).

Still a work in progress, i.e. only supports a relatively small part
of all the functionality that Git offers, although enough for two real
world projects so far. Hopefully easy to extend, although perhaps the
API design could be improved--suggestions welcome.

