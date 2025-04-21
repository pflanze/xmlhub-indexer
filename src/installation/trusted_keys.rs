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
const TRUSTED_KEYS: &[TrustedKey] = &[TrustedKey {
    public_key: "d66e4b948019efb4e96bac79e90ec4234f2831777ae5bcf5a7e306519796b30e",
    creator: "cjaege@bs-mbpas-0130",
    owner: "Christian Jaeger (Mac) <ch@christianjaeger.ch>",
}];

/// Return the full trusted info on a key if trusted
pub fn get_trusted_key(public_key: &str) -> Option<&'static TrustedKey> {
    TRUSTED_KEYS
        .into_iter()
        .find(|trusted| trusted.public_key == public_key)
}
