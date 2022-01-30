// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 primitives related to time types.

use {
    bcder::{
        decode::{Constructed, Malformed, Primitive, Source},
        encode::{PrimitiveContent, Values},
        Mode, Tag,
    },
    chrono::{Datelike, TimeZone, Timelike},
    std::{
        fmt::{Display, Formatter},
        io::Write,
        ops::{Add, Deref},
        str::FromStr,
    },
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

impl From<chrono::DateTime<chrono::Utc>> for Time {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self::UtcTime(UtcTime(t))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Zone {
    Utc,
    Offset(chrono::FixedOffset),
}

impl Display for Zone {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Utc => f.write_str("Z"),
            Self::Offset(offset) => f.write_str(format!("{}", offset).replace(":", "").as_str()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneralizedTime {
    time: chrono::NaiveDateTime,
    timezone: Zone,
}

impl GeneralizedTime {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_primitive_if(Tag::GENERALIZED_TIME, |prim| Self::from_primitive(prim))
    }

    pub fn from_primitive<S: Source>(prim: &mut Primitive<S>) -> Result<Self, S::Err> {
        let data = prim.take_all()?;

        Self::parse(data.as_ref()).map_err(|e| e.into())
    }

    /// Parse GeneralizedTime string data.
    pub fn parse(data: &[u8]) -> Result<Self, bcder::decode::Error> {
        if data.len() != "YYYYMMDDHHMMSSZ".len() {
            return Err(Malformed);
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
            return Err(Malformed);
        }

        if let chrono::LocalResult::Single(dt) = chrono::Utc.ymd_opt(year, month, day) {
            if let Some(dt) = dt.and_hms_opt(hour, minute, second) {
                Ok(Self {
                    time: dt.naive_utc(),
                    timezone: Zone::Utc,
                })
            } else {
                Err(Malformed)
            }
        } else {
            Err(Malformed)
        }
    }
}

impl ToString for GeneralizedTime {
    fn to_string(&self) -> String {
        format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}{}",
            self.time.year(),
            self.time.month(),
            self.time.day(),
            self.time.hour(),
            self.time.minute(),
            self.time.second(),
            self.timezone,
        )
    }
}

impl From<GeneralizedTime> for chrono::DateTime<chrono::Utc> {
    fn from(gt: GeneralizedTime) -> Self {
        match gt.timezone {
            Zone::Utc => chrono::DateTime::<chrono::Utc>::from_utc(gt.time, chrono::Utc),
            Zone::Offset(offset) => {
                chrono::DateTime::<chrono::Utc>::from_utc(gt.time.add(offset), chrono::Utc)
            }
        }
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generalized_time() -> Result<(), bcder::decode::Error> {
        let gt = GeneralizedTime {
            time: chrono::NaiveDateTime::from_timestamp(1643510772, 0),
            timezone: Zone::Utc,
        };
        assert_eq!(gt.to_string(), "20220130024612Z");

        let gt = GeneralizedTime {
            time: chrono::NaiveDateTime::from_timestamp(1643510772, 0),
            timezone: Zone::Offset(chrono::FixedOffset::east(3600)),
        };
        assert_eq!(gt.to_string(), "20220130024612+0100");

        let gt = GeneralizedTime {
            time: chrono::NaiveDateTime::from_timestamp(1643510772, 0),
            timezone: Zone::Offset(chrono::FixedOffset::west(7200)),
        };
        assert_eq!(gt.to_string(), "20220130024612-0200");

        let gt = GeneralizedTime::parse(b"20220129133742Z")?;
        assert_eq!(gt.time.year(), 2022);
        assert_eq!(gt.time.month(), 1);
        assert_eq!(gt.time.day(), 29);
        assert_eq!(gt.time.hour(), 13);
        assert_eq!(gt.time.minute(), 37);
        assert_eq!(gt.time.second(), 42);
        assert_eq!(gt.time.nanosecond(), 0);
        assert_eq!(format!("{}", gt.timezone), "Z");

        assert_eq!(gt.to_string(), "20220129133742Z");

        // TODO support fractional seconds.
        assert!(GeneralizedTime::parse(b"20220129133742.333Z").is_err());

        // TODO support timezone offset.
        assert!(GeneralizedTime::parse(b"20220129133742-0800").is_err());
        assert!(GeneralizedTime::parse(b"20220129133742+1000").is_err());

        // TODO support fractional seconds with timezone offset.
        assert!(GeneralizedTime::parse(b"20220129133742.333-0800").is_err());

        Ok(())
    }

    #[test]
    fn generalized_time_invalid() {
        assert!(GeneralizedTime::parse(b"").is_err());
        assert!(GeneralizedTime::parse(b"abcd").is_err());
        assert!(GeneralizedTime::parse(b"2022").is_err());
        assert!(GeneralizedTime::parse(b"202201").is_err());
        assert!(GeneralizedTime::parse(b"20220130").is_err());
        assert!(GeneralizedTime::parse(b"2022013012").is_err());
        assert!(GeneralizedTime::parse(b"202201301230").is_err());
        assert!(GeneralizedTime::parse(b"20220130123015").is_err());
        assert!(GeneralizedTime::parse(b"20220130123015a").is_err());
        assert!(GeneralizedTime::parse(b"20220130123015-").is_err());
        assert!(GeneralizedTime::parse(b"20220130123015+").is_err());
        assert!(GeneralizedTime::parse(b"20220130123015+01").is_err());
        assert!(GeneralizedTime::parse(b"20220130123015+01000").is_err());
        assert!(GeneralizedTime::parse(b"20220130123015+0100a").is_err());
        assert!(GeneralizedTime::parse(b"20220130123015-01000").is_err());
        assert!(GeneralizedTime::parse(b"20220130123015-0100a").is_err());
    }
}
