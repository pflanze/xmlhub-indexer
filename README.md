# Index builder for the XML Hub

This is a tool to build an index of the files in the (non-public) "XML
Hub" Git repository of the cEvo group at the D-BSSE, ETH Zurich.

It is meant to be run after new XML files are uploaded into the XML
Hub Git repository, or existing files are removed or changed. It is
designed to be run manually on a checkout of the XML Hub repository,
or periodically via a service that allows to do that (e.g.`launchd` on
macOS). It might also be possible to integrate into CI (continuous
integration) of the Git hosting platform (GitLab / GitHub) so that it
is automatically run immediately when xmlhub receives changes.

## Installation

### Download

You can download pre-built binaries from
[xmlhub-indexer-binaries](https://cevo-git.ethz.ch/cevo-resources/xmlhub-indexer-binaries),
currently for macOS on ARM only as it's currently built on the machine
of the person making a release from (I may change this).

### From source

You can build the program yourself:

 1. Install the Rust toolchain via [rustup](https://rustup.rs/) (or
    other means as per [Install
    Rust](https://www.rust-lang.org/tools/install)).

 2. Open a fresh terminal (to make sure your `PATH` environment
    variable is updated to include the location where the Rust tooling
    resides). Get the program source code: 

        git clone git@cevo-git.ethz.ch:cevo-resources/xmlhub-indexer.git

 3. Go to the top level directory of your Git checkout, i.e. `cd
    xmlhub-indexer` after you ran the above command. Then run `cargo
    install --path .`. You should now be able to call the program via
    `xmlhub-build-index`.

    Alternatively, if the above fails for some reason, you can run
    `cargo build --release` (still from the xmlhub-indexer directory),
    and then copy the file at `target/release/xmlhub-build-index` to a
    place where you can reach it (either to a directory that's listed
    in your `PATH` environment variable, or some other place convenient
    to you).

## Usage

Once installed, you should be able to run the program via

    xmlhub-build-index --help
    
or giving it the path to your local git clone of the
[xmlhub](https://cevo-git.ethz.ch/cevo-resources/xmlhub) repository

    xmlhub-build-index path/to/your/checkout/of/xmlhub

which will update the `README.md` and `README.html` files in the
xmlhub directory and commit any changes to those. You can then `git
push` the changes. There is also a `--push` option that lets
`xmlhub-build-index` do the latter, too.

Note that `xmlhub-build-index` only reads files that have the suffix
`.xml` *and are added to the repository*. If you create a new XML
file, first run `git add path/to/your.xml` before running
`xmlhub-build-index`.

If there are errors in any of the XML files, `xmlhub-build-index` will not
overwrite the files by default, and instead just writes the errors to
he terminal. If you wish to proceed anyway (because you want to see
the errors in the browser, or even push them to the repo for others to
see), give the `--write-errors` (or the short variant, `-w`)
option. This will commit (and push) the errors by default, give
`--no-commit-errors` if you don't want that.

There are also `--open` (always open) and `--open-if-changed` options
which open your browser on the generated `README.html` file (see the
`--help` text for details). I recommend to use one of them as this
file can more easily be read than the view on GitLab, and it can show
problems like missing attributes in red, while GitLab strips the red
marking and just shows those parts in black. Also you can verify
things before committing if you give `--no-commit-errors`.

If you want to just run the conversion periodically you could use this
command line (the order of options doesn't actually matter, the
program executes them in the sensible order anyway; you can also use
the short options shown in the `--help` text instead):

    xmlhub-build-index path/to/your/checkout/of/xmlhub --pull --write-errors --open-if-changed --push

or if you run it on a repository you never use interactively (e.g. on
a server), this is more fail proof for automatic action (but deletes
local changes to the repo!):

    xmlhub-build-index path/to/your/checkout/of/xmlhub --batch

Running this will pull, convert, write the output even if there are
errors, and if there were changes, commit and push them back to the
Git repository, and open your browser. You could put that into a
(shell) script that you could run without having to remember the
arguments.

(It should also be possible to set up a CI pipeline (continuous
integration) on GitLab to run `xmlhub-build-index` automatically whenever
the xmlhub repository receives changes on GitLab, but maybe that's too
much magic and not worth the additional complexity.)

Besides those settings changeable via command line options, there are
various others hard coded but defined and easily changeable near the
top of the main program file,
[`xmlhub-build-index.rs`](src/bin/xmlhub-build-index.rs), that can
easily be changed, although you will need to recompile the program for
that--see the [From source](#from-source) and [Maintaining and
changing the program](#Maintaining-and-changing-the-program) sections.

## Details

### Parsing

Note: the ultimate truth is the code, but this should be correct at
the time of writing.

  - Every attribute in an XML file is expected to be in a separate XML
    comment. This makes it unambiguous where one attribute starts and
    ends, and obviates the need for any other more complicated
    format. No escaping is needed or possible; but the string "-->" or
    already "--" cannot be part of an attribute value.
    
  - Spaces (really any kind of whitespace, including newlines) are
    trimmed off values on both ends.  Space in the middle is
    normalized to a single space for each block of whitespace, except
    for single-item attributes (`AttributeKind::String`) if the
    `normalize_whitespace` setting is `false` (which is actually the
    setting used for all such attributes at the time of this writing).
  
  - The string "NA" is treated the same as the empty string, both are
    treated as not available, leading to an error report if the value
    is required.
    
  - Attribute keys (names) are case insensitive. They should be
    specified in the program source code (in `METADATA_SPECIFICATION`)
    in proper spelling, since that is what is used for display in the
    index pages.
    
  - Attribute order in the XML file doesn't matter; for display, the
    one given in `METADATA_SPECIFICATION` is used.
    
  - Attributes that take lists of values can be split on whatever one
    configures in the `input_separator` field of
    `AttributeKind::StringList`, but the "," makes most sense as
    splitting on space doesn't work when values can also have version
    numbers like for the "Packages" attribute--if space is used both
    to separate package names from package version but also to split
    between package entries, it would be ambiguous.
    
  - Attribute indexing can lowercase the values for uniformity; this
    is useful for keywords, not so much for package names and other
    things where casing is more relevant; it can be changed via the
    `use_lowercase` field of `AttributeIndexing::Index`.

## XML Hub maintenance

If you're the XML Hub maintainer, these are points to look out for:

  * You may want to sign up to get notification emails from GitLab
    when there are changes to XML Hub. Or run the indexer periodically
    (see "run the conversion periodically" above), perhaps
    automatically via some automatic job runner (e.g.`launchd` on
    macOS, todo: test).

  * Verify that there are no errors (nothing in red in the rendering
    of the local README.html file).

  * Check the indices for the attributes that can have multiple values
    (like "Keywords") for entries that have spaces in them: those may
    be missing a comma where the space is (the writer probably meant
    the words as individual keywords). You can also check the file
    info box for the changed file(s) instead, those attributes that
    can have multiple values show each individual value between double
    quotes (like `“base”, “BDSKY”, “feast”`), making missing commas
    obvious (the same would be shown as `“base BDSKY feast”`).

## Maintaining and changing the program

This program is written in the [Rust](https://rust-lang.org)
programming language, which is geared towards making programs that are
largely bug free and stable for a long time. Chances are that no fixes
will be needed for many years. But even if true, of course XML Hub
might have new requirements requiring changes to the program, too.

To change the program, you need to first be able to build it from
source, as per the [From source](#from-source) section above.

While working on it, it's more practical to build and run it in one
go. Run it e.g. like this (the `--` are needed to stop processing of
options by `cargo` itself; `--no-commit` if you want to verify the
output before committing to it):

    cargo run --bin xmlhub-build-index -- ~/tmp/xmlhub/ --no-commit

You will also want to use an IDE for editing Rust code. The standard
recommendation is VSCode with the Rust-Analyzer extension (see [Rust
in Visual Studio
Code](https://code.visualstudio.com/docs/languages/rust)). Many other
editors provide Rust support, too (check for LSP support and make sure
rust-analyzer is installed/enabled). You could even edit in a
bare-bones editor, but then you will only get error reporting when
compiling via `cargo build`, and you may not get any help with method
name completion, type display, function documentation display etc. So
if you want to do larger changes, you should definitely use an editor
with good Rust development support.

The main program file is
[`src/bin/xmlhub-build-index.rs`](src/bin/xmlhub-build-index.rs). It shouldn't be
necessary to change anything in the other files.

The thing you most likely want to update is the
`METADATA_SPECIFICATION` constant. The entries here describe which
metadata keys are valid, and how they are parsed and indexed. You can
introduce new metadata types simply by adding/changing
`AttributeSpecification` entries here.

The `main` function, which is the last item in the
[`src/bin/xmlhub-build-index.rs`](src/bin/xmlhub-build-index.rs) file
(search for "fn main" if your IDE doesn't make it easy to find), is
what is called when invoking the program. It's a good idea to start
here, to see what things the program does in which order--although the
function `build_index`, which is called from `main`, carries out the
actual index building.

Use IDE functionality (try context menu (right mouse click)) to jump
to the definitions of functions or methods that are called. If you
want to read through the whole code, you should be able to read
through the file from top to bottom, the code is roughly ordered in a
way that makes that sensible. The code is split into sections
separated with `// ====...` to make it clearer what belongs
together. Some other interesting starting points might be searching
for `let toplevel_section` for all the sections, or `let intro` for
the intro text.

There is a file with settings that are shared between the
`xmlhub-build-index` and `make-xmlhub-indexer-release` programs:
[`xmlhub_indexer_defaults.rs`](src/xmlhub_indexer_defaults.rs). You
find docs on the fields in the [declaration of
`CheckoutContext`](src/checkout_context.rs).

Thanks to the stringent type checking during compilation, you can be
rather confident that you didn't break anything when you got it to
compile. You can also check whether the Git diff of the resulting
output files written to the xmlhub repository looks sensible (you need
to use a program that is good at showing changes within the long lines
of HTML code that the files contain).

By default, errors are shown without a backtrace. If you want to know
which location an error originates from, run the default debug build
(i.e. do *not* use the `--release` option) with the environment
variable setting `RUST_BACKTRACE=1`, e.g.

    RUST_BACKTRACE=1 cargo run --bin xmlhub-build-index -- ~/tmp/xmlhub

### Release process

After making changes to the xmlhub-indexer, the
changes should be published back to GitLab so that others can get
them. This entails the following--*but note the next subsection*, you
don't have to do this manually!:

- Deciding on the new version name. Semantic versioning is used, which
  means that the first digit in the version number is incremented when
  incompatible changes were made. Changing the generated output is
  understood as incompatible here: if two maintainers used two
  different versions that produce different output, and they
  alternatively run `xmlhub-build-index`, then the xmlhub repository would
  receive changed index files each time, even when the inputs (the XML
  files) didn't change (i.e. they would overwrite each other's outputs
  and create new Git commits every time, spamming the Git
  history). `xmlhub-build-index`, when it commits changes to the xmlhub
  repo, automatically adds version information to the commit message,
  and before indexing verifies that the version of the last commit is
  lower or compatible, to prevent that situation. Version numbers
  should start with the letter "v" (but that's optional) then 1 to 3
  non-negative integers joined with a "."

- Creating a git tag with the new version name, and "git push"-ing
  back both the tag and the current branch (master) to the
  xmlhub-indexer repository on GitLab. Git tags can be created with
  PGP signatures to allow others to verify the authenticity of a
  release.

- Rebuilding the binary, then copying it from the `target/release/`
  directory into the correct folder in the checkout of the
  `xmlhub-indexer-binaries` repository, adding and committing it there
  (preferably with information about the host and environment in which
  it was built), and if signing, also adding a git tag, then pushing
  branch and tag also back to GitLab.

#### `make-xmlhub-indexer-release`

In addition to `xmlhub-build-index`, the xmlhub-indexer repository
contains a `make-xmlhub-indexer-release` program which carries out all
of the above steps automatically. It runs tests and collects
information, then shows a summary of the changes that will be carried
out and asks for confirmation before acting.

Use the `--help` option for more information. It is recommended to use
both the `--push` and `--sign` options. From within the
"xmlhub-indexer" directory run:

    cargo run --bin make-xmlhub-indexer-release -- --sign --push

Caveats:

- Unlike `xmlhub-build-index`, it does not currently have a `--pull`
  option; if you use the `--push` option and the "git push" step fails
  due to the remote (GitLab) having been updated by someone else in
  the meantime, you're expected to pull (and verify) the changes
  yourself, then re-run the `make-xmlhub-indexer-release` program.

- It currently only publishes binaries when run on macOS or Linux, and
  it has not been tested on Windows at all.

For making signed tags (using the `--sign` option), you need a PGP/gpg
key. If you don't have one, in a terminal, run

    gpg --generate-key

then follow the instructions. When done, `cd` into your checkout of
the `xmlhub-indexer-binaries` repository, then run

    gpg --export -a "your name or fingerprint" > keys/your-name.asc
    git add .
    git commit -m "add key"
    git push

so that others can then run `git --import key/your-name.asc`
from their checkout once and then run `git tag -v v123` to verify the
authenticity of the v123 version. To know whether the key is actually
yours, both people can run `gpg --fingerprint "your name"` (or leave
away the name string and get all keys) and then compare the
fingerprints (hex number string with spaces) on the screen.

While care has been taken to try to make the `xmlhub-build-index` source
code easy to understand (newbie-friendly), for
`make-xmlhub-indexer-release` that goal has been dropped; it does use
some advanced Rust features.

### Quick Rust primer

Code comments are introduced by `//` or `///` (or `//!`) and go to the
end of the line. `///` comments (and `//!` for module documentation)
are parsed by the automatic documentation system (`cargo doc`) or the
IDE, they represent documentation for the item that follows them. `//`
comments are not tied to any item, and are only visible to the reader
of the source code.

`struct TypeName` declares a type that is a data structure with fields
(similar to a class or dict in other languages), `enum TypeName`
declares a type that has a number of alternative types, one for each
named branch. `impl TypeName` implements methods on either kind of
type. (Less used: `impl InterfaceName for TypeName` implements the
methods specified in `trait InterfaceName` (can be in another file and
imported) for the type `TypeName`.)

`?` means that the expression to the left can produce an error, and
that this error should be returnd from the current function at this
point (the current function must have a `Result<..>`
type). `.with_context(anyhow!("..."))` adds context information to the
error before it is being returned, letting the user know in which
context it happened.

`iter()` and `into_iter()` create an iterator over the items in the
object, the first leaves the object intact, the second consumes the
object (which can be more performant but means the object to the left
cannot be used any longer afterwards). To get back from an iterator to
a materialized data structure (can be a vector (`Vec`), but also other
things like hash tables (`HashMap`)), `.collect()` can be used; what
kind of thing `collect` should create is largely inferred from the
context, but sometimes it has to be helped by giving the type after
`::` as in `.collect::<Result<_>>()`--the `_` here is a placeholder
for any type, so this example means, "collect into a result of
something I let you infer", meaning, it's indicating that there can be
errors, that `collect` should be prepared to stop processing if one
happens during iteration and then return that error.

Rust code generally doesn't use the `return` keyword, the value of the
expression that was evaluated last in a function is automatically
returned from the function (you must omit the `;` after that
expression, or the last expression becomes the empty expression after
the `;`, which returns `()`, the empty tuple, meaning "no
value"). This is not only true for functions, but also nexted blocks
`{ ... }`, like for `if .. { } else { }` statements, and pretty much
everywhere (Rust is an expression-oriented language).

Rust has pattern matching syntax via the `match` keyword, but also `if
let ... = ...`. The former allows multiple alternatives, the latter
only one (and an `else` fallback).

`|x, y| x + y` or `|x, y| { x + y }` are anonymous functions
(closures), taking x and y as arguments. The `{ .. }` are optional
unless you need multiple expressions separated by `;`.

Rust checks types of values when compiling the program, not when
running it; top-level functions (those defined via the `fn` keyword)
need type declarations for its arguments and return value--for the
former, the types are given after the `:`, for the return type, after
the `->`. Same is true for structures (declarations via the keyword
`struct`), and for top-level constants (`const` and `static`). OTOH,
for variables inside functions, and the argument and return types for
anonymous functions, the types can most often be inferred
automatically and declarations are hence largely optional.

Putting `&` left of an expression means to share the place on the
right (i.e. use a *reference* to the value instead of passing along
the value itself), without consuming the value in that place. By
default, Rust passes values on by *moving* them, meaning the original
place (variable or struct field) will not have it anymore afterwards;
that's why you want to use `&` if you want to share, but not consume
the value. But references returned via `&` are only valid as long as
the place they are referring to still exists; if the compiler can't
see that this is the case, it will refuse to compile the program. You
can use `.clone()` to make a copy of the value if a `&` wouldn't work
but moving the value is also not OK. Some types, including number
types, and references, are cheap to copy and hence implicitly cloned
(they have the `Copy` trait) instead of moved. If a variable has a
reference the value is needed, the dereferencing operator `*` can be
put left of the place holding it to follow the reference to the value
(e.g. `*id`; for `Copy` types, this is equivalent to `id.clone()`).

Why these complications in the above paragraph? Rust does not use an
automatic garbage collector that observes where references to values
are used while the program is running (in most languages with GC,
*all* values are referred to by reference implicitly, and they live on
the GC heap as long as there is at least one reference); instead all
values live in one particular place (variable or struct field), and
when that place goes away, so does the value. To let other pieces of
the code access the value there, sharing via reference must be done
explicitly via `&` as described. The programmer must decide with some
foresight which is the place with access to the value that stays
around the longest (or chain of places, as values can be moved from
place to place, but no references are allowed to exist while a move
takes place--again, the compiler checks this).

`.into()` is a method that converts the object to the left into the
type that is expected by the place that receives the result of the
current expression; this can be e.g. a conversion from a reference to
a string (`&String`) to a new string instance (`String`), which clones
the referenced string. Or it could be from a shared subsection, called
slice, of a string (`&str`) to a new string instance (`String`). Or
other conversions not used here. `.as_ref()` achieves something
similar specifically for representing the object on the left as a
reference of the expected type.

Identifiers followed by a `!` are macro calls; those can do fancier
things than function calls, like destructuring format strings during
compilation to safely embed values. `#[derive ..]` syntax are another
kind of macros that implement features on the following data structure
(Debug is the ability to be formatted in debugging contexts, Clone to
allow clone() to be called, PartialEq for equality comparison etc.)

## Help

You can get formatted documentation for the programs and their
dependencies except for the standard library (which is at [standard
library docs](https://doc.rust-lang.org/std/) instead) via running
`cargo doc --bin xmlhub-build-index --open` (or `cargo doc --bins
--open` which will build all program's docs but may open the browser
on the wrong one). These should open your web browser, alternatively
find the generated html files in `target/doc/`.

You can also use the IDE functionality to see a function's docs, or
follow from a function call to the function's source code.

The original author of this program, Christian Jaeger
<ch@christianjaeger.ch>, is happy to help if you have questions.

You can also get help via Google, GPT, the [standard library
docs](https://doc.rust-lang.org/std/), the `##rust` channel on
[IRC](https://libera.chat/), the [the Rust programming language users
forum](https://users.rust-lang.org/), and various other places.

There's also the [Get started with
Rust](https://www.rust-lang.org/learn) page, with a link to "the book"
and other info.
