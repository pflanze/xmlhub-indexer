# Index builder for the XML Hub

This is a tool to build an index of the files in the (non-public) "XML
Hub" Git repository of the cEvo group at the D-BSSE, ETH Zurich.

It is meant to be run after new XML files are uploaded into the XML
Hub Git repository, or existing files are removed or changed. It can
be run manually on a checkout of the XML Hub repository, or it could
be integrated into CI (continuous integration) of the Git hosting
platform (GitLab / GitHub) so that it is automatically run when
changes are uploaded.

## Installation

### Downloads

You can download pre-built binaries (currently for macOS, built
manually) from
[xmlhub-indexer-binaries](https://cevo-git.ethz.ch/cevo-resources/xmlhub-indexer-binaries).

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
    `xmlhub-indexer`.

    Alternatively, if the above fails for some reason, you can run
    `cargo build --release` (still from the xmlhub-indexer directory),
    and then copy the file at `target/release/xmlhub-indexer` to a
    place where you can reach it (either to a directory that's listed
    in your `PATH` environment variable, or some other place convenient
    to you).

## Usage

Once installed, you should be able to run the program via

    xmlhub-indexer --help
    
or giving it the path to your local git clone of the
[xmlhub](https://cevo-git.ethz.ch/cevo-resources/xmlhub) repository

    xmlhub-indexer path/to/your/checkout/of/xmlhub

which will update the `file_index.md` and `file_index.html` files in
the xmlhub directory. You can then commit and "git push" the
changes. There are also `--commit` and `--push` options that let
xmlhub-indexer do that itself:

    xmlhub-indexer path/to/your/checkout/of/xmlhub --commit --push

There are also `--open` (always open) and `--open-if-changed` options
which open your browser on the generated `file_index.html` file (see
the `--help` text for details). I recommend to use one of them as this
file can more easily be read, and it can show problems like missing
attributes in red, while GitLab strips the red marking and just shows
those parts in black. Also you can verify things before committing.

If you want to just run the conversion periodically you could use this
command line (the order of options doesn't actually matter, the
program executes them in the sensible order anyway; you can also use
the short options shown in the `--help` text instead):

    xmlhub-indexer path/to/your/checkout/of/xmlhub --pull --write-errors --open-if-changed --commit --push

Running this will pull, convert, write the output even if there are
errors, and if there were changes, commit and push them back to the
Git repository, and open your browser. You could put that into a
(shell) script that you could run without having to remember the
arguments.

(It should also be possible to set up a CI pipeline (continuous
integration) on GitLab to run xmlhub-indexer automatically whenever
the xmlhub repository receives changes on GitLab, but maybe that's too
much magic and not worth the additional complexity.)

## Details

### Parsing

Note: the ultimate truth is the code, but this should be correct at
the time of writing.

  - Every attribute in an XML file is expected to be in another XML
    comment. This makes it unambiguous where one starts and ends, and
    obviates the need for another more complicated format. No escaping
    or anything is done (other than what XML itself requires) (this
    means that the string "-->" cannot be part of an attribute value).
    
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
    configures in the `separator` field of
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
    automatically via some automatic job runner (todo: figure out
    details).

  * Make sure that there are no errors (nothing in red in the
    rendering of the local html file).

  * Check the indices for the attributes that can have multiple values
    (like "Keywords") for entries that have spaces in them: those may
    be missing a comma where the space is (the writer probably meant
    the words as individual keywords). You can also check the file
    info box for the changed file(s) instead, those attributes that
    can have multiple values shows each individual entry between
    double quotes, making missing commas obvious.

## Maintaining and changing the program

This program is written in the [Rust](https://rust-lang.org)
programming language, which is geared towards making programs that are
largely bug free and stable for a long time. Chances are that no fixes
will be needed for many years. But even if true, of course XML Hub
might have new requirements requiring changes to the program anyway.

To change the program, you need to first be able to build it from
source, as per the [From source](#from-source) section above.

While working on it, it's more practical to build and run it in one
go. Run it e.g. like this (the `--` are needed to stop processing of
options by `cargo` itself):

    cargo run -- ~/tmp/xmlhub/ --commit

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
[`src/bin/xmlhub-indexer.rs`](src/bin/xmlhub-indexer.rs). It shouldn't be
necessary to change anything in the other files.

The thing you most likely want to update is the
`METADATA_SPECIFICATION` constant. The entries here describe which
metadata keys are valid, and how they are parsed and indexed. You can
introduce new metadata types simply by adding/changing
`AttributeSpecification` entries here.

The `main` function, which is the last item in the
[`src/bin/xmlhub-indexer.rs`](src/bin/xmlhub-indexer.rs) file (search
for "fn main" if your IDE doesn't make it easy to find), is what
is called when invoking the program. It's a good idea to start here,
to see what things the program does in which order. Use IDE
functionality (try context menu (right mouse click)) to jump to the
definitions of functions or methods that are called. If you want to
read through the whole code, you should be able to read through the
file from top to bottom, the code is roughly ordered in a way that
makes that sensible. The code is split into sections separated with
`// ====...` to make it clearer what belongs together. Some other
interesting starting points might be searching for `let
toplevel_section` for all the sections, or `let intro` for the intro
text.

Thanks to the stringent type checking during compilation, you can be
rather confident that you didn't break anything when you got it to
compile. You can also check whether the Git diff of the resulting
output files written to the xmlhub repository looks sensible (you need
to use a program that is good at showing changes within the long lines
of HTML code that the files contain).

### Quick Rust primer

Code comments are introduced by `//` or `///` (or `//!`) and go to the
end of the line. `///` comments (and `//!` for module documentation)
are parsed by the automatic documentation system (`cargo doc`) or the
IDE, they represent documentation for the item that follows them. `//`
comments are not tied to any item, and are only visible to the reader
of the source code.

`struct` declares a type that is a data structure with fields (similar
to a class or dict in other languages), `enum` declares a type that
has a number of alternative types, one for each named branch. `impl`
implements methods on either kind of type.

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

You can get formatted documentation for this program and all of its
dependencies except for the standard library (which is at [standard
library docs](https://doc.rust-lang.org/std/) instead) via running
`cargo doc` and then opening the `target/doc` folder in your browser
(e.g. `firefox target/doc/`). (You can also use the IDE functionality
to see a function's docs, or follow from a function call to the
function's source code.)

The original author of this program, Christian Jaeger
<ch@christianjaeger.ch>, is happy to help if you have questions.

You can also get help via Google, GPT, the [standard library
docs](https://doc.rust-lang.org/std/), the `##rust` channel on
[IRC](https://libera.chat/), the [the Rust programming language users
forum](https://users.rust-lang.org/), and various other places.

There's also the [Get started with
Rust](https://www.rust-lang.org/learn) page, with a link to "the book"
and other info.
