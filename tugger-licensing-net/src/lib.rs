// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    spdx::LicenseReq,
    std::{fmt::Write, io::Read},
    tugger_licensing::{ComponentFlavor, LicenseFlavor, LicensedComponent, SourceLocation},
    url::Url,
};

const LICENSE_TEXT_URL: &str =
    "https://raw.githubusercontent.com/spdx/license-list-data/master/text/{}.txt";

pub const DEFAULT_LICENSE_PREAMBLE: &str =
    "This product contains software subject to licenses as described below.";

/// Obtain the SPDX license text for a given license ID.
pub fn get_spdx_license_text(
    client: &reqwest::blocking::Client,
    license_id: &str,
) -> Result<String> {
    let url = Url::parse(&LICENSE_TEXT_URL.replace("{}", license_id))?;

    let mut response = client.get(url.clone()).send()?;
    if response.status() != 200 {
        return Err(anyhow!("HTTP {} from {}", response.status(), url));
    }
    let mut license_text = String::new();
    response.read_to_string(&mut license_text)?;

    Ok(license_text)
}

/// Resolve an `spdx::LicenseReq` to license text.
///
/// Only works for valid SPDX licenses. If an exception is present, that exception
/// text will be concatenated to the original license's text.
pub fn license_requirement_to_license_text(
    client: &reqwest::blocking::Client,
    req: &LicenseReq,
) -> Result<String> {
    if let Some(id) = &req.license.id() {
        let mut texts = vec![get_spdx_license_text(client, id.name)?];

        if let Some(exception) = &req.exception {
            texts.push(get_spdx_license_text(client, exception.name)?);
        }

        Ok(texts.join("\n"))
    } else {
        Err(anyhow!("license requirement must have valid SPDX license"))
    }
}

/// Obtain texts of SPDX licenses which apply to this component.
///
/// If non-SPDX license identifiers are present, they are ignored. Consider
/// calling `is_spdx()` to ensure only SPDX license identifiers are used.
pub fn licensed_component_spdx_license_texts(
    component: &LicensedComponent,
    client: &reqwest::blocking::Client,
) -> Result<Vec<String>> {
    let reqs = match component.license() {
        LicenseFlavor::Spdx(expression) => expression.requirements().collect::<Vec<_>>(),
        LicenseFlavor::OtherExpression(expression) => expression
            .requirements()
            .filter(|req| req.req.license.id().is_some())
            .collect::<Vec<_>>(),
        LicenseFlavor::None | LicenseFlavor::PublicDomain | LicenseFlavor::Unknown(_) => vec![],
    };

    reqs.iter()
        .map(|req| license_requirement_to_license_text(client, &req.req))
        .collect::<Result<Vec<_>>>()
}

/// Generates text content describing the licensing of all components.
///
/// `preamble` is introductory text that will be printed before the automatically
/// generated text.
pub fn generate_aggregate_license_text<'a>(
    components: impl Iterator<Item = &'a LicensedComponent>,
    client: &reqwest::blocking::Client,
    preamble: &str,
) -> Result<String> {
    let mut text = preamble.to_string();
    writeln!(&mut text)?;
    writeln!(&mut text)?;

    for component in components.filter(|c| !matches!(c.license(), LicenseFlavor::None)) {
        let title = format!("{} License", component.name());
        writeln!(&mut text, "{}", title)?;
        writeln!(&mut text, "{}", "=".repeat(title.len()))?;
        writeln!(&mut text)?;
        writeln!(
            &mut text,
            "This product contains the {} {}.",
            component.name(),
            match component.flavor() {
                ComponentFlavor::Generic => "software",
                ComponentFlavor::Library => "library",
                ComponentFlavor::RustCrate => "Rust crate",
                ComponentFlavor::PythonPackage => "Python package",
            }
        )?;
        writeln!(&mut text)?;
        match component.source_location() {
            SourceLocation::NotSet => {}
            SourceLocation::Url(url) => {
                writeln!(
                    &mut text,
                    "The source code for {} can be found at\n{}",
                    component.name(),
                    url
                )?;
                writeln!(&mut text)?;
            }
        }
        match component.license() {
            LicenseFlavor::Spdx(expression) => {
                writeln!(
                    &mut text,
                    "The SPDX license expression of this component is\n\"{}\".",
                    expression
                )?;
            }
            LicenseFlavor::OtherExpression(expression) => {
                writeln!(
                    &mut text,
                    "The SPDX license expression of this component is \n\"{}\".",
                    expression
                )?;
            }
            LicenseFlavor::PublicDomain => {
                writeln!(&mut text, "This component is in the public domain.")?;
            }
            LicenseFlavor::None => {}
            LicenseFlavor::Unknown(terms) => {
                writeln!(
                    &mut text,
                    "This component is licensed according to {}",
                    terms.join(", ")
                )?;
            }
        }

        writeln!(&mut text)?;
        if component.license_texts().is_empty() {
            writeln!(
                &mut text,
                "{}",
                licensed_component_spdx_license_texts(&component, &client)?.join("\n")
            )?;
        } else {
            writeln!(&mut text, "{}", component.license_texts().join("\n"))?;
        }
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use {super::*, tugger_licensing::LicensedComponents};

    #[test]
    fn test_spdx_license_texts() -> Result<()> {
        let client = tugger_common::http::get_http_client()?;

        let c = LicensedComponent::new_spdx("foo", "Apache-2.0")?;
        assert_eq!(licensed_component_spdx_license_texts(&c, &client)?.len(), 1);

        let c = LicensedComponent::new_spdx("foo", "Apache-2.0 OR MPL-2.0")?;
        assert_eq!(licensed_component_spdx_license_texts(&c, &client)?.len(), 2);

        let c = LicensedComponent::new_spdx("foo", "Apache-2.0 AND MPL-2.0")?;
        assert_eq!(licensed_component_spdx_license_texts(&c, &client)?.len(), 2);

        let c = LicensedComponent::new_spdx("foo", "Apache-2.0 WITH LLVM-exception")?;
        assert_eq!(licensed_component_spdx_license_texts(&c, &client)?.len(), 1);

        Ok(())
    }

    #[test]
    fn test_generate_aggregate_license_text() -> Result<()> {
        let client = tugger_common::http::get_http_client()?;

        let mut c = LicensedComponents::default();
        c.add_spdx_only_component(LicensedComponent::new_spdx("foo", "Apache-2.0")?)?;
        c.add_spdx_only_component(LicensedComponent::new_spdx("bar", "MIT")?)?;

        generate_aggregate_license_text(c.iter_components(), &client, DEFAULT_LICENSE_PREAMBLE)?;

        Ok(())
    }
}
