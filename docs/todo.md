# List of planned changes

The way how we currently provide the index over the documents is by
storing the generated index file(s) back into the Git repository with
the documents. This looked like a sensible way to keep serving XML Hub
simple: the same Git repository that holds the documents also comes
with the index, and both are visible on Gitlab/GitHub at the same
place.

But this approach poses some inherent problems:

- The index needs to be regenerated to see the updates, which makes
  running the [tool](tool.html) locally all the more important.
  
- Document changes/additions/removals and subsequent index updates
  usually receive their own commits, which clutters the
  history. Worse, there is some usage complexity in that the tool
  needs to make commits itself to store its own version number to
  ensure no two tools overwrite each other's index updates in a loop.
  Even worse, concurrent changes to the repository from different
  computers lead to changed index files in both branches, and the
  subsequent merge leads to merge conflicts on those index files;
  either users need to be instructed how to deal with those, or
  alternatively the tool would have to be extended to carry out all
  git operations, which feels evil considering users may be relatively
  new to Git ("when do I have to do something on my own in Git, and
  when does the tool do it? Am I even using Git at that point?")

- What users generally want to see is just the index over *all*
  repositories, not just a single one; having an index in every
  repository will be confusing.

So we want to change the approach in that we stop storing the index
back into the Git repositories. The repositories will then be
*manually* manageable, with direct Git commands. Index regeneration
will happen on the server(s), or on the fly when run on the user's
local machine for preview.

