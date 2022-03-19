// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Code entitlements handling. */

#[cfg(test)]
mod test {
    use {
        crate::{AppleSignable, CodeSigningSlot},
        anyhow::anyhow,
        anyhow::Result,
        goblin::mach::Mach,
        plist::{Date, Uid, Value},
        std::{
            process::Command,
            time::{Duration, SystemTime},
        },
    };

    const DER_EMPTY_DICT: &[u8] = &[112, 5, 2, 1, 1, 176, 0];
    const DER_BOOL_FALSE: &[u8] = &[
        112, 15, 2, 1, 1, 176, 10, 48, 8, 12, 3, 107, 101, 121, 1, 1, 0,
    ];
    const DER_BOOL_TRUE: &[u8] = &[
        112, 15, 2, 1, 1, 176, 10, 48, 8, 12, 3, 107, 101, 121, 1, 1, 255,
    ];
    const DER_INTEGER_0: &[u8] = &[
        112, 15, 2, 1, 1, 176, 10, 48, 8, 12, 3, 107, 101, 121, 2, 1, 0,
    ];
    const DER_INTEGER_NEG1: &[u8] = &[
        112, 15, 2, 1, 1, 176, 10, 48, 8, 12, 3, 107, 101, 121, 2, 1, 255,
    ];
    const DER_INTEGER_1: &[u8] = &[
        112, 15, 2, 1, 1, 176, 10, 48, 8, 12, 3, 107, 101, 121, 2, 1, 1,
    ];
    const DER_INTEGER_42: &[u8] = &[
        112, 15, 2, 1, 1, 176, 10, 48, 8, 12, 3, 107, 101, 121, 2, 1, 42,
    ];
    const DER_STRING_EMPTY: &[u8] = &[112, 14, 2, 1, 1, 176, 9, 48, 7, 12, 3, 107, 101, 121, 12, 0];
    const DER_STRING_VALUE: &[u8] = &[
        112, 19, 2, 1, 1, 176, 14, 48, 12, 12, 3, 107, 101, 121, 12, 5, 118, 97, 108, 117, 101,
    ];
    const DER_ARRAY_EMPTY: &[u8] = &[112, 14, 2, 1, 1, 176, 9, 48, 7, 12, 3, 107, 101, 121, 48, 0];
    const DER_ARRAY_FALSE: &[u8] = &[
        112, 17, 2, 1, 1, 176, 12, 48, 10, 12, 3, 107, 101, 121, 48, 3, 1, 1, 0,
    ];
    const DER_ARRAY_TRUE_FOO: &[u8] = &[
        112, 22, 2, 1, 1, 176, 17, 48, 15, 12, 3, 107, 101, 121, 48, 8, 1, 1, 255, 12, 3, 102, 111,
        111,
    ];
    const DER_DICT_EMPTY: &[u8] = &[
        112, 14, 2, 1, 1, 176, 9, 48, 7, 12, 3, 107, 101, 121, 176, 0,
    ];
    const DER_DICT_BOOL: &[u8] = &[
        112, 26, 2, 1, 1, 176, 21, 48, 19, 12, 3, 107, 101, 121, 176, 12, 48, 10, 12, 5, 105, 110,
        110, 101, 114, 1, 1, 0,
    ];
    const DER_MULTIPLE_KEYS: &[u8] = &[
        112, 37, 2, 1, 1, 176, 32, 48, 8, 12, 3, 107, 101, 121, 1, 1, 0, 48, 9, 12, 4, 107, 101,
        121, 50, 1, 1, 255, 48, 9, 12, 4, 107, 101, 121, 51, 2, 1, 42,
    ];

    /// Signs a binary with custom entitlements XML and retrieves the entitlements DER.
    ///
    /// This uses Apple's `codesign` executable to sign the current binary then uses
    /// our library for extracting the entitlements DER that it generated.
    fn sign_and_get_entitlements_der(value: &Value) -> Result<Vec<u8>> {
        let this_exe = std::env::current_exe()?;

        let temp_dir = tempfile::tempdir()?;

        let in_path = temp_dir.path().join("original");
        let entitlements_path = temp_dir.path().join("entitlements.xml");
        std::fs::copy(&this_exe, &in_path)?;
        {
            let mut fh = std::fs::File::create(&entitlements_path)?;
            value.to_writer_xml(&mut fh)?;
        }

        let args = vec![
            "--verbose".to_string(),
            "--force".to_string(),
            // ad-hoc signing since we don't care about a CMS signature.
            "-s".to_string(),
            "-".to_string(),
            "--generate-entitlement-der".to_string(),
            "--entitlements".to_string(),
            format!("{}", entitlements_path.display()),
            format!("{}", in_path.display()),
        ];

        let status = Command::new("codesign").args(args).output()?;
        if !status.status.success() {
            return Err(anyhow!("codesign invocation failure"));
        }

        // Now extract the data from the Apple produced code signature.

        let signed_exe = std::fs::read(&in_path)?;
        let mach = Mach::parse(&signed_exe)?;
        let macho = match mach {
            Mach::Binary(macho) => macho,
            Mach::Fat(multiarch) => multiarch.get(0).expect("unable to read fat binary"),
        };

        let signature = macho
            .code_signature()?
            .expect("unable to find code signature");

        let slot = signature
            .find_slot(CodeSigningSlot::EntitlementsDer)
            .expect("unable to find der entitlements blob");

        if slot.data.len() < 9 {
            return Err(anyhow!("blob is too short"));
        }

        Ok(slot.payload()?.to_vec())
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn apple_der_entitlements_encoding() -> Result<()> {
        // `codesign` prints "unknown exception" if we attempt to serialize a plist where
        // the root element isn't a dict.
        let mut d = plist::Dictionary::new();

        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_EMPTY_DICT
        );

        d.insert("key".into(), Value::Boolean(false));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_BOOL_FALSE
        );

        d.insert("key".into(), Value::Boolean(true));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_BOOL_TRUE
        );

        d.insert("key".into(), Value::Integer(0u32.into()));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_INTEGER_0
        );

        d.insert("key".into(), Value::Integer((-1i32).into()));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_INTEGER_NEG1
        );

        d.insert("key".into(), Value::Integer(1u32.into()));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_INTEGER_1
        );

        d.insert("key".into(), Value::Integer(42u32.into()));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_INTEGER_42
        );

        // Floats fail to encode to DER.
        d.insert("key".into(), Value::Real(0.0f32.into()));
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());

        d.insert("key".into(), Value::Real((-1.0f32).into()));
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());

        d.insert("key".into(), Value::Real(1.0f32.into()));
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());

        d.insert("key".into(), Value::String("".into()));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_STRING_EMPTY
        );

        d.insert("key".into(), Value::String("value".into()));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_STRING_VALUE
        );

        // Uids fail to encode with `UidNotSupportedInXmlPlist` message.
        d.insert("key".into(), Value::Uid(Uid::new(0)));
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());

        d.insert("key".into(), Value::Uid(Uid::new(1)));
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());

        d.insert("key".into(), Value::Uid(Uid::new(42)));
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());

        // Date doesn't appear to work due to
        // `Failed to parse entitlements: AMFIUnserializeXML: syntax error near line 6`. Perhaps
        // a bug in the plist crate?
        d.insert(
            "key".into(),
            Value::Date(Date::from(SystemTime::UNIX_EPOCH)),
        );
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());
        d.insert(
            "key".into(),
            Value::Date(Date::from(
                SystemTime::UNIX_EPOCH + Duration::from_secs(86400 * 365 * 30),
            )),
        );
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());

        // Data fails to encode to DER with `unknown exception`.
        d.insert("key".into(), Value::Data(vec![]));
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());
        d.insert("key".into(), Value::Data(b"foo".to_vec()));
        assert!(sign_and_get_entitlements_der(&Value::Dictionary(d.clone())).is_err());

        d.insert("key".into(), Value::Array(vec![]));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_ARRAY_EMPTY
        );

        d.insert("key".into(), Value::Array(vec![Value::Boolean(false)]));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_ARRAY_FALSE
        );

        d.insert(
            "key".into(),
            Value::Array(vec![Value::Boolean(true), Value::String("foo".into())]),
        );
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_ARRAY_TRUE_FOO
        );

        let mut inner = plist::Dictionary::new();
        d.insert("key".into(), Value::Dictionary(inner.clone()));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_DICT_EMPTY
        );

        inner.insert("inner".into(), Value::Boolean(false));
        d.insert("key".into(), Value::Dictionary(inner.clone()));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_DICT_BOOL
        );

        d.insert("key".into(), Value::Boolean(false));
        d.insert("key2".into(), Value::Boolean(true));
        d.insert("key3".into(), Value::Integer(42i32.into()));
        assert_eq!(
            sign_and_get_entitlements_der(&Value::Dictionary(d.clone()))?,
            DER_MULTIPLE_KEYS
        );

        Ok(())
    }
}
