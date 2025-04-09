use std::fmt::Display;

pub struct TrustedKey {
    public_key: &'static str,
    creator: &'static str,
    owner: &'static str,
}

impl Display for TrustedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            public_key,
            creator,
            owner,
        } = self;
        f.write_fmt(format_args!("key {public_key} by {owner} ({creator})"))
    }
}

/// The keys that are trusted to sign binaries safe to install:
/// `[(public_key, creator, owner)]`
const TRUSTED_KEYS: &[(&str, &str, &str)] = &[(
    "d66e4b948019efb4e96bac79e90ec4234f2831777ae5bcf5a7e306519796b30e",
    "cjaege@bs-mbpas-0130",
    "Christian Jaeger (Mac) <ch@christianjaeger.ch>",
)];

/// Return the full trusted info on a key if trusted
pub fn get_trusted_key(public_key: &str) -> Option<TrustedKey> {
    let (public_key, creator, owner) = TRUSTED_KEYS
        .into_iter()
        .find(|(key, _, _)| *key == public_key)?;
    Some(TrustedKey {
        public_key,
        creator,
        owner,
    })
}
