// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use bcder::{ConstOid, Oid};

/// Common Name (CN)
///
/// 2.5.4.3
pub const OID_COMMON_NAME: ConstOid = Oid(&[85, 4, 3]);

/// Country Name (C)
///
/// 2.5.4.6
pub const OID_COUNTRY_NAME: ConstOid = Oid(&[85, 4, 6]);

/// Organization Name (O)
///
/// 2.5.4.10
pub const OID_ORGANIZATION_NAME: ConstOid = Oid(&[85, 4, 10]);

/// Organizational Unit Name (OU)
///
/// 2.5.4.11
pub const OID_ORGANIZATIONAL_UNIT_NAME: ConstOid = Oid(&[85, 4, 11]);
