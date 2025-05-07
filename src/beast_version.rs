use std::{
    fmt::Display,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context};
use roxmltree::Document;

/// What would normally be called the "major" version in a semantic
/// version is called the product version for BEAST, since BEAST 1, 2,
/// .. are "totally different products".
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum BeastProductVersion {
    One,
    Two,
    Future(u16),
}

impl TryFrom<u16> for BeastProductVersion {
    type Error = anyhow::Error;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => bail!("not a BEAST product version number: {value}"),
            1 => Ok(BeastProductVersion::One),
            2 => Ok(BeastProductVersion::Two),
            n => Ok(BeastProductVersion::Future(n)),
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct BeastVersion {
    pub product: BeastProductVersion,
    /// This is the BEAST2 "major" number, if product is `Two`, e.g. "2.7.3" => 7
    pub major: Option<u16>,
    /// The full version string
    pub string: String,
}

impl Display for BeastVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.string)
    }
}

impl FromStr for BeastVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let string = s.trim().to_owned();
        let parts: Vec<&str> = string.split('.').collect();
        if parts.len() < 2 {
            bail!("not a BEAST version number, misses a '.': {string:?}")
        } else {
            let product_str = parts[0];
            let product_num = u16::from_str(product_str).with_context(|| {
                anyhow!(
                    "{string:?} is not a BEAST version number, the product number part \
                     {product_str:?} is not an unsigned integer"
                )
            })?;
            let product = BeastProductVersion::try_from(product_num)
                .with_context(|| anyhow!("parsing version number string {string:?}"))?;
            Ok(Self {
                product,
                major: match product {
                    BeastProductVersion::Two => {
                        let major_str = parts[1];
                        let major_num = u16::from_str(major_str).with_context(|| {
                            anyhow!(
                                "not a BEAST version number, the BEAST-major number part is \
                     not an unsigned integer: {:?} in {string:?}",
                                major_str
                            )
                        })?;
                        Some(major_num)
                    }
                    _ => None,
                },
                string,
            })
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum GetBeastVersionError {
    #[error("not a BEAST file, the root element is not a <beast> element")]
    NotABeastFile,
    #[error("<beast> element is missing the `version` attribute")]
    MissingVersionAttribute,
    #[error("error parsing BEAST version number")]
    BeastVersionParseError(#[from] anyhow::Error),
}

pub fn get_beast_version(document: &Document) -> Result<BeastVersion, GetBeastVersionError> {
    let root_element = document.root_element();
    if root_element.tag_name().name() != "beast" {
        // XX check the namespace?
        return Err(GetBeastVersionError::NotABeastFile);
    }
    if let Some(version) = root_element.attribute("version") {
        Ok(BeastVersion::from_str(version)?)
    } else {
        Err(GetBeastVersionError::MissingVersionAttribute)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CheckBeastVersionError {
    #[error(
        "only BEAST2 XML files are supported, unless you provide the --ignore-version \
         option; file {source_path:?} indicates version {version}"
    )]
    NotABeast2File {
        source_path: PathBuf,
        version: String,
    },
    #[error("can't get BEAST version number")]
    GetBeastVersionError(#[from] GetBeastVersionError),
}

pub fn check_beast_version<P: AsRef<Path>>(
    document: &Document,
    source_path: P,
    ignore_1_2_version: bool,
) -> Result<BeastVersion, CheckBeastVersionError> {
    let beast_version = get_beast_version(document)?;

    if !ignore_1_2_version && beast_version.product != BeastProductVersion::Two {
        return Err(CheckBeastVersionError::NotABeast2File {
            source_path: source_path.as_ref().to_owned(),
            version: beast_version.string,
        });
    }
    Ok(beast_version)
}
