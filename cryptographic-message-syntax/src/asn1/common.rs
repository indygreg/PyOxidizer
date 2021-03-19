// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    bcder::{
        decode::{Constructed, Malformed, Primitive, Source},
        encode::{PrimitiveContent, Values},
        Mode, Tag,
    },
    chrono::{Datelike, TimeZone, Timelike},
    std::{io::Write, ops::Deref, str::FromStr},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Time {
    UtcTime(UtcTime),
    GeneralTime(GeneralizedTime),
}

impl Time {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_primitive(|tag, prim| match tag {
            Tag::UTC_TIME => Ok(Self::UtcTime(UtcTime::from_primitive(prim)?)),
            Tag::GENERALIZED_TIME => Ok(Self::GeneralTime(GeneralizedTime::from_primitive(prim)?)),
            _ => Err(Malformed.into()),
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        match self {
            Self::UtcTime(utc) => (Some(utc.encode()), None),
            Self::GeneralTime(gt) => (None, Some(gt.encode())),
        }
    }
}

impl AsRef<chrono::DateTime<chrono::Utc>> for Time {
    fn as_ref(&self) -> &chrono::DateTime<chrono::Utc> {
        match self {
            Self::UtcTime(dt) => dt.deref(),
            Self::GeneralTime(dt) => dt.deref(),
        }
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Time {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self::UtcTime(UtcTime(t))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneralizedTime(chrono::DateTime<chrono::Utc>);

impl Deref for GeneralizedTime {
    type Target = chrono::DateTime<chrono::Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl GeneralizedTime {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_primitive_if(Tag::GENERALIZED_TIME, |prim| Self::from_primitive(prim))
    }

    pub fn from_primitive<S: Source>(prim: &mut Primitive<S>) -> Result<Self, S::Err> {
        let data = prim.take_all()?;

        if data.len() != "YYYYMMDDHHMMSSZ".len() {
            return Err(Malformed.into());
        }

        let year = i32::from_str(std::str::from_utf8(&data[0..4]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;
        let month = u32::from_str(std::str::from_utf8(&data[4..6]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;
        let day = u32::from_str(std::str::from_utf8(&data[6..8]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;
        let hour = u32::from_str(std::str::from_utf8(&data[8..10]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;
        let minute = u32::from_str(std::str::from_utf8(&data[10..12]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;
        let second = u32::from_str(std::str::from_utf8(&data[12..14]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;

        if data[14] != b'Z' {
            return Err(Malformed.into());
        }

        if let chrono::LocalResult::Single(dt) = chrono::Utc.ymd_opt(year, month, day) {
            if let Some(dt) = dt.and_hms_opt(hour, minute, second) {
                Ok(Self(dt))
            } else {
                Err(Malformed.into())
            }
        } else {
            Err(Malformed.into())
        }
    }
}

impl ToString for GeneralizedTime {
    fn to_string(&self) -> String {
        format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}Z",
            self.0.year(),
            self.0.month(),
            self.0.day(),
            self.0.hour(),
            self.0.minute(),
            self.0.second()
        )
    }
}

impl PrimitiveContent for GeneralizedTime {
    const TAG: Tag = Tag::GENERALIZED_TIME;

    fn encoded_len(&self, _: Mode) -> usize {
        self.to_string().len()
    }

    fn write_encoded<W: Write>(&self, _: Mode, target: &mut W) -> Result<(), std::io::Error> {
        target.write_all(self.to_string().as_bytes())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UtcTime(chrono::DateTime<chrono::Utc>);

impl UtcTime {
    /// Obtain a new instance with now as the time.
    pub fn now() -> Self {
        Self(chrono::Utc::now())
    }

    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_primitive_if(Tag::UTC_TIME, |prim| Self::from_primitive(prim))
    }

    pub fn from_primitive<S: Source>(prim: &mut Primitive<S>) -> Result<Self, S::Err> {
        let data = prim.take_all()?;

        if data.len() != "YYMMDDHHMMSSZ".len() {
            return Err(Malformed.into());
        }

        let year = i32::from_str(std::str::from_utf8(&data[0..2]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;

        let year = if year >= 50 { year + 1900 } else { year + 2000 };

        let month = u32::from_str(std::str::from_utf8(&data[2..4]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;
        let day = u32::from_str(std::str::from_utf8(&data[4..6]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;
        let hour = u32::from_str(std::str::from_utf8(&data[6..8]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;
        let minute = u32::from_str(std::str::from_utf8(&data[8..10]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;
        let second = u32::from_str(std::str::from_utf8(&data[10..12]).map_err(|_| Malformed)?)
            .map_err(|_| Malformed)?;

        if data[12] != b'Z' {
            return Err(Malformed.into());
        }

        if let chrono::LocalResult::Single(dt) = chrono::Utc.ymd_opt(year, month, day) {
            if let Some(dt) = dt.and_hms_opt(hour, minute, second) {
                Ok(Self(dt))
            } else {
                Err(Malformed.into())
            }
        } else {
            Err(Malformed.into())
        }
    }
}

impl ToString for UtcTime {
    fn to_string(&self) -> String {
        format!(
            "{:02}{:02}{:02}{:02}{:02}{:02}Z",
            self.0.year() % 100,
            self.0.month(),
            self.0.day(),
            self.0.hour(),
            self.0.minute(),
            self.0.second()
        )
    }
}

impl Deref for UtcTime {
    type Target = chrono::DateTime<chrono::Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PrimitiveContent for UtcTime {
    const TAG: Tag = Tag::UTC_TIME;

    fn encoded_len(&self, _: Mode) -> usize {
        self.to_string().len()
    }

    fn write_encoded<W: Write>(&self, _: Mode, target: &mut W) -> Result<(), std::io::Error> {
        target.write_all(self.to_string().as_bytes())
    }
}
