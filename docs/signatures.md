# Signature verification

We are using cryptographic signatures to assert that the source code
and the binaries that we publish are from us and haven't been tampered
with. These signatures are providing end-to-end security as opposed to just HTTPS security to
the repositories. There are two kinds of signatures, the first one is for manual verification (steps described below) of the initial download, the second is handled automatically by the `xmlhub` tool. To clarify this distinction, both are described here. I.e. after you have done the manual verification for the initial installation, future upgrades via `xmlhub upgrade` are automatically verified to be from us, too.

If you assume that the Git repository can only be written to by us (we try to make sure that's the case) and that the Git hosting service does not have security issues (we hope that's the case but there have been issues in the past), then carrying out the manual verification doesn't yield you appreciable additional security. We provide the possibility for more highly security-sensitive environments. You can of course also review and compile the [source code]({xmlhubIndexerRepoUrl}) yourself.

(We may manage to have a [reproducible build](https://en.wikipedia.org/wiki/Reproducible_build) in the future to also allow others to verify that the binaries we provide are in fact produced from our source code. Also, browsers might implement [local filesystem access](https://wicg.github.io/file-system-access/) in the future, at which point we can replace the local executable with a web page doing the local work. Also, you can work with XML Hub completely without local tool support. Also, we could make a traditional web site user interface for file management that works with the current browsers if warranted--the reason we went with a local tool is that working directly with Git was deemed a practical and "low tech" approach that allows parties to better see what's going on.)

## Manual verification (PGP signatures)

We create PGP signatures on Git tags in the source ({xmlhubIndexerRepoLink}) and binaries ({xmlhubIndexerBinariesRepoLink}) repositories.

These use the standard git tag tooling, but you need GnuPG installed for it to work. The steps are:

1. Install GnuPG, either from (ETH shop XX)  or from upstream source XX.

2. Import our PGP public keys:

        gpg --import xmlhub-indexer-binaries/keys/*.asc

3. Run:

        cd xmlhub-indexer-binaries
        gitk # then see which tag you want to verify, copy it
        git tag -v $tag_name

    This should indicate a good signature using, currently, the key with fingerprint `399736E864B395FE46C0DF67BACC825BB13D4A0A`. To check that this is the right fingerprint:

        gpg --with-colons --check-sigs 399736E864B395FE46C0DF67BACC825BB13D4A0A

    Amongst other lines this should print one with `sig:!:` at the beginning then `7312F47D9436FBF8C3F80CF2748247966F366AE9`, which is the fingerprint of Christian Jaeger's personal key signing the above key; that key fingerprint can be found on his personal website via a Google query for `"7312F47D9436FBF8C3F80CF2748247966F366AE9"` (include the quotes!).

## App signatures (automatic)

Our release process creates `xmlhub.info.sig` files in the {xmlhubIndexerBinariesRepoLink} repository. These are created with a separate key created specifically for this purpose, but you don't have to care about these yourself: the `xmlhub` tool automatically checks these signatures when running `xmlhub upgrade` to ensure that the executable being installed is from us. The tool embeds a list of the public keys for the keys that it trusts--you can see the list in the file `src/installation/trusted_keys.rs` in the {xmlhubIndexerRepoLink} repository. We keep these keys on the machines we use for building the released executables, only.

