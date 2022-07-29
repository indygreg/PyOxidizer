// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        binary::{
            ElfBinaryInfo, ElfSection, ElfSymbol, X86InstructionCounts, X86_INSTRUCTION_CODES,
        },
        import::IndexedPackage,
    },
    anyhow::{anyhow, Context, Result},
    indoc::indoc,
    rusqlite::{params, Connection, Transaction},
    std::{
        collections::{BTreeMap, BTreeSet, HashMap, HashSet},
        path::Path,
    },
};

const SCHEMA: &[&str] = &[
    indoc! {"
        CREATE TABLE package (
            id INTEGER PRIMARY KEY,
            name TEXT,
            version TEXT,
            source_url TEXT
        )"},
    "CREATE UNIQUE INDEX package_source_url ON package(source_url)",
    indoc! {"
        CREATE TABLE package_file (
            id INTEGER PRIMARY KEY,
            package_id INTEGER REFERENCES package(id) ON DELETE CASCADE,
            path TEXT,
            size INTEGER
        )
    "},
    indoc! {"
        CREATE TABLE elf_file (
            id INTEGER PRIMARY KEY,
            package_file_id INTEGER REFERENCES package_file(id) ON DELETE CASCADE,
            class INTEGER NOT NULL,
            data_encoding INTEGER NOT NULL,
            os_abi INTEGER NOT NULL,
            abi_version INTEGER NOT NULL,
            object_file_type INTEGER NOT NULL,
            machine INTEGER NOT NULL,
            entry_address TEXT NOT NULL,
            flags INTEGER NOT NULL,
            program_header_size INTEGER NOT NULL,
            program_header_count INTEGER NOT NULL,
            section_header_size INTEGER NOT NULL,
            section_header_count INTEGER NOT NULL,
            plt_relocations_size INTEGER,
            rel_relocations_size INTEGER,
            rela_relocations_size INTEGER,
            string_table_size INTEGER,
            init_function_address TEXT,
            termination_function_address TEXT,
            shared_object_name TEXT,
            dynamic_flags INTEGER,
            dynamic_flags_1 INTEGER,
            runpath TEXT,
            relocations_count INTEGER,
            relocations_addends_count INTEGER
        )
    "},
    indoc! {"
        CREATE TABLE elf_section (
            elf_file_id INTEGER REFERENCES elf_file(id) ON DELETE CASCADE,
            number INTEGER NOT NULL,
            name TEXT,
            section_type INTEGER NOT NULL,
            flags INTEGER NOT NULL,
            address TEXT NOT NULL,
            offset INTEGER NOT NULL,
            size INTEGER NOT NULL,
            link INTEGER NOT NULL,
            info INTEGER NOT NULL,
            address_alignment INTEGER NOT NULL,
            entity_size INTEGER NOT NULL
        )
    "},
    indoc! {"
        CREATE UNIQUE INDEX elf_file_section_index
        ON elf_section(elf_file_id, number)
    "},
    indoc! {"
        CREATE TABLE elf_file_needed_library (
            elf_file_id INTEGER REFERENCES elf_file(id) ON DELETE CASCADE,
            name TEXT
        )
    "},
    // Symbol names are highly duplicated when multiple packages are imported. So
    // create a dedicated table for the string values rather than storing duplicate
    // columns everywhere.
    indoc! {"
        CREATE TABLE symbol_name (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            name_demangled TEXT
        )
    "},
    indoc! {"
        CREATE UNIQUE INDEX symbol_name_name
        ON symbol_name(name)
    "},
    indoc! {"
        CREATE TABLE elf_symbol (
            id INTEGER PRIMARY KEY,
            elf_file_id INTEGER REFERENCES elf_file(id) ON DELETE CASCADE,
            section_index INTEGER NOT NULL,
            symbol_index INTEGER NOT NULL,
            name_id INTEGER REFERENCES symbol_name(id) ON DELETE CASCADE,
            symbol_type INTEGER NOT NULL,
            binding INTEGER NOT NULL,
            visibility INTEGER NOT NULL,
            section_header_index INTEGER NOT NULL,
            value TEXT NOT NULL,
            size TEXT NOT NULL,
            version_filename TEXT,
            version_version TEXT
        )
    "},
    indoc! {"
        CREATE TABLE elf_file_x86_instruction_count (
            elf_file_id INTEGER REFERENCES elf_file(id) ON DELETE CASCADE,
            instruction TEXT,
            occurrences INTEGER
        )
    "},
    indoc! {"
        CREATE INDEX elf_file_x86_instruction_count_name
        ON elf_file_x86_instruction_count(instruction)
    "},
    indoc! {"
        CREATE TABLE elf_file_x86_register_count (
            elf_file_id INTEGER REFERENCES elf_file(id) ON DELETE CASCADE,
            register TEXT,
            occurrences INTEGER
        )
    "},
    indoc! {"
        CREATE TABLE elf_file_x86_base_register_count (
            elf_file_id INTEGER REFERENCES elf_file(id) ON DELETE CASCADE,
            register TEXT,
            occurrences INTEGER
        )
    "},
    // Onto views.
    indoc! {"
        CREATE VIEW v_package_file AS
            SELECT
                package.name AS package_name,
                package.version AS package_version,
                package.source_url AS package_source_url,
                package_file.path AS file_path,
                package_file.size AS file_size
            FROM package, package_file
            WHERE package_file.package_id = package.id
    "},
    indoc! {"
        CREATE VIEW v_elf_needed_library AS
            SELECT
                package.name AS package_name,
                package.version AS package_version,
                package.source_url AS package_source_url,
                package_file.path AS package_path,
                elf_file_needed_library.name AS library_name
            FROM
                package, package_file, elf_file, elf_file_needed_library
            WHERE
                package_file.package_id = package.id
                AND elf_file.package_file_id = package_file.id
                AND elf_file_needed_library.elf_file_id = elf_file.id
    "},
    indoc! {"
        CREATE VIEW v_elf_symbol AS
            SELECT
                package.name AS package_name,
                package.version AS package_version,
                package.source_url AS package_source_url,
                package_file.path AS package_path,
                elf_symbol.section_index AS elf_section_index,
                symbol_name.name AS elf_symbol_name,
                symbol_name.name_demangled AS elf_symbol_name_demangled,
                elf_symbol.symbol_type AS elf_symbol_type,
                elf_symbol.binding AS elf_symbol_binding,
                elf_symbol.visibility AS elf_symbol_visibility,
                elf_symbol.section_header_index AS elf_symbol_section_header_index,
                elf_symbol.value AS elf_symbol_value,
                elf_symbol.size AS elf_symbol_size,
                elf_symbol.version_filename AS elf_symbol_version_filename,
                elf_symbol.version_version AS elf_symbol_version_version
            FROM package, package_file, elf_file, symbol_name, elf_symbol
            WHERE
                package_file.package_id = package.id
                AND elf_file.package_file_id = package_file.id
                AND elf_symbol.elf_file_id = elf_file.id
                AND elf_symbol.name_id = symbol_name.id
    "},
    indoc! {"
        CREATE VIEW v_package_elf_file AS
            SELECT
                package.id AS package_id,
                package.name AS package_name,
                package.version AS package_version,
                package_file.id AS package_file_id,
                elf_file.id AS elf_file_id,
                elf_file.object_file_type AS elf_object_file_type,
                elf_file.machine AS elf_machine,
                package_file.path AS package_file_path
            FROM package, package_file, elf_file
            WHERE
                package_file.package_id = package.id
                AND elf_file.package_file_id = package_file.id
            ORDER BY
                package_name ASC,
                package_version ASC,
                package_file_path ASC
    "},
    indoc! {"
        CREATE VIEW v_package_instruction_count AS
            SELECT
                v_package_elf_file.package_id,
                v_package_elf_file.package_name,
                v_package_elf_file.package_version,
                elf_file_x86_instruction_count.instruction,
                SUM(elf_file_x86_instruction_count.occurrences) AS counts
            FROM v_package_elf_file, elf_file_x86_instruction_count
            WHERE
                elf_file_x86_instruction_count.elf_file_id=v_package_elf_file.elf_file_id
            GROUP BY package_id, instruction
            ORDER BY
                package_name ASC,
                package_version ASC,
                instruction ASC
    "},
    "PRAGMA user_version=1",
];

/// A connection to a SQLite database to hold indexed data.
pub struct DatabaseConnection {
    conn: Connection,
}

impl DatabaseConnection {
    /// Open a new connection to a SQLite database in memory.
    pub fn new_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;

        let slf = Self { conn };
        slf.init()?;

        Ok(slf)
    }

    /// Open a new connection to a SQLite database in a filesystem path.
    pub fn new_path(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).context("opening SQLite connection")?;

        let slf = Self { conn };
        slf.init()?;

        Ok(slf)
    }

    fn init(&self) -> Result<()> {
        // WAL journal is a reasonable default for most environments.
        self.conn.pragma_update(None, "journal_mode", "WAL")?;

        // This disables a lot of safety and makes it really easy to corrupt the database. But
        // it also makes things very fast. Since this software is intended for ad-hoc querying,
        // trading safety for performance seems reasonable.
        self.conn.pragma_update(None, "synchronous", "OFF")?;

        self.conn.pragma_update(None, "cache_size", "-100000")?;

        let user_version: usize = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;

        match user_version {
            0 => {
                for statement in SCHEMA {
                    self.conn
                        .execute(statement, [])
                        .with_context(|| format!("initializing schema: {}", statement))?;
                }
            }
            1 => {}
            _ => {
                return Err(anyhow!(
                    "unexpected user_version; database likely corrupted"
                ));
            }
        }

        Ok(())
    }

    /// Execute a function in the context of a SQLite transaction.
    pub fn with_transaction(
        &mut self,
        f: impl FnOnce(DatabaseTransaction) -> Result<()>,
    ) -> Result<()> {
        let txn = self.conn.transaction()?;

        let txn = DatabaseTransaction { txn };

        f(txn)
    }

    /// Obtain the set of all known package URLs.
    pub fn package_urls(&self) -> Result<HashSet<String>> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT source_url FROM package")
            .context("preparing package URLs query")?;

        let res = statement.query_map([], |row| {
            let url: String = row.get(0)?;

            Ok(url)
        })?;

        Ok(res.collect::<Result<HashSet<_>, _>>()?)
    }

    pub fn packages_with_filename(&self, filename: &str) -> Result<Vec<(String, String, String)>> {
        let mut statement = self
            .conn
            .prepare_cached(indoc! {"
            SELECT package_name, package_version, file_path
            FROM v_package_file
            WHERE file_path LIKE ?
            ORDER BY package_name ASC, package_version ASC, file_path ASC
        "})
            .context("preparing packages with filename query")?;

        let res = statement.query_map(params![format!("%/{}", filename)], |row| {
            let package: String = row.get(0)?;
            let version: String = row.get(1)?;
            let path: String = row.get(2)?;

            Ok((package, version, path))
        })?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    /// Obtain the number of indexed ELF files.
    pub fn elf_file_count(&self) -> Result<u64> {
        let mut statement = self
            .conn
            .prepare_cached(indoc! {"
            SELECT COUNT(*) FROM elf_file
        "})
            .context("preparing elf file count query")?;

        let mut rows = statement.query(params![])?;

        let row = rows.next()?.ok_or_else(|| anyhow!("should have a row"))?;

        Ok(row.get(0)?)
    }

    /// Obtain a list of all known ELF files.
    pub fn elf_files(&self) -> Result<Vec<(String, String, String)>> {
        let mut statement = self
            .conn
            .prepare_cached(indoc! {"
            SELECT package_name, package_version, package_file_path
            FROM v_package_elf_file
            ORDER BY package_name ASC, package_version ASC, package_file_path ASC
        "})
            .context("preparing elf files query")?;

        let res = statement.query_map([], |row| {
            let package: String = row.get(0)?;
            let version: String = row.get(1)?;
            let path: String = row.get(2)?;

            Ok((package, version, path))
        })?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    /// Obtain counts of ELF sections by their name.
    ///
    /// Emitted records are in order of descending counts.
    pub fn elf_file_section_counts_global(&self) -> Result<Vec<(String, u64)>> {
        let mut statement = self
            .conn
            .prepare_cached(indoc! {"
                SELECT name, COUNT(*) AS counts
                FROM elf_section
                GROUP BY name
                ORDER BY counts DESC, name ASC
            "})
            .context("preparing elf file section counts global query")?;

        let res = statement.query_map([], |row| {
            let section: String = row.get(0)?;
            let count: u64 = row.get(1)?;

            Ok((section, count))
        })?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    /// Obtain all IFUNC symbols on a per-file basis.
    ///
    /// Keys have (package, version, file path).
    pub fn elf_file_ifuncs(&self) -> Result<BTreeMap<(String, String, String), BTreeSet<String>>> {
        let mut statement = self
            .conn
            .prepare_cached(indoc! {"
            SELECT package_name, package_version, package_path, elf_symbol_name
            FROM v_elf_symbol
            WHERE elf_symbol_type = ?
            ORDER BY package_source_url ASC
        "})
            .context("preparing elf file ifuncs query")?;

        let mut h = BTreeMap::new();

        let mut res = statement.query(params![object::elf::STT_GNU_IFUNC])?;

        while let Some(row) = res.next()? {
            let package: String = row.get(0)?;
            let version: String = row.get(1)?;
            let path: String = row.get(2)?;
            let symbol: String = row.get(3)?;

            let key = (package.clone(), version.clone(), path.clone());

            let entry = h.entry(key).or_insert_with(BTreeSet::new);
            entry.insert(symbol);
        }

        Ok(h)
    }

    /// Find ELF files defining a specified symbol.
    ///
    /// Returns (package, version, file path).
    pub fn elf_files_defining_symbol(&self, symbol: &str) -> Result<Vec<(String, String, String)>> {
        let mut statement = self
            .conn
            .prepare_cached(indoc! {"
            SELECT DISTINCT package_name, package_version, package_path
            FROM v_elf_symbol
            WHERE
                elf_symbol_type IN (?, ?, ?)
                AND elf_symbol_section_header_index != ?
                AND elf_symbol_name = ?
            ORDER BY package_name ASC, package_version ASC, package_path ASC
        "})
            .context("preparing elf files defining symbol query")?;

        let res = statement.query_map(
            params![
                object::elf::STT_NOTYPE,
                object::elf::STT_FUNC,
                object::elf::STT_OBJECT,
                object::elf::SHN_UNDEF,
                symbol,
            ],
            |row| {
                let package: String = row.get(0)?;
                let version: String = row.get(1)?;
                let path: String = row.get(2)?;

                Ok((package, version, path))
            },
        )?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    /// Find all ELF files importing a given named symbol.
    ///
    /// Returns a tuple of (package, package version, filename).
    pub fn elf_files_importing_symbol(
        &self,
        symbol: &str,
    ) -> Result<Vec<(String, String, String)>> {
        let mut statement = self
            .conn
            .prepare_cached(indoc! {"
            SELECT DISTINCT package_name, package_version, package_path
            FROM v_elf_symbol
            WHERE
                elf_symbol_section_header_index = ?
                AND elf_symbol_name = ?
            ORDER BY package_name ASC, package_version ASC, package_path ASC
        "})
            .context("preparing elf files importing symbol query")?;

        let res = statement.query_map(params![object::elf::SHN_UNDEF, symbol], |row| {
            let package: String = row.get(0)?;
            let version: String = row.get(1)?;
            let path: String = row.get(2)?;

            Ok((package, version, path))
        })?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    /// Obtain the total number of instructions for all indexed binary files.
    ///
    /// Returns tuples are (package, version, file path, count).
    pub fn x86_instruction_counts_by_binary(
        &self,
        instruction: Option<&str>,
    ) -> Result<Vec<(String, String, String, u64)>> {
        let (extra, params) = if let Some(instruction) = instruction {
            (
                "AND elf_file_x86_instruction_count.instruction = ?",
                vec![instruction.to_lowercase()],
            )
        } else {
            ("", vec![])
        };

        let mut statement = self
            .conn
            .prepare_cached(&format!(
                indoc! {"
                SELECT
                    package.name AS package_name,
                    package.version AS package_version,
                    package_file.path AS package_path,
                    SUM(elf_file_x86_instruction_count.occurrences) AS count
                FROM
                    package, package_file, elf_file, elf_file_x86_instruction_count
                WHERE
                    package_file.package_id = package.id
                    AND elf_file.package_file_id = package_file.id
                    AND elf_file_x86_instruction_count.elf_file_id = elf_file.id
                    {}
                GROUP BY
                    elf_file_x86_instruction_count.elf_file_id
                ORDER BY
                    package_name ASC,
                    package_version ASC,
                    package_path ASC
        "},
                extra,
            ))
            .context("preparing x86 instruction counts by package query")?;

        let res = statement.query_map(
            rusqlite::params_from_iter(params.iter().map(|x| x.as_str())),
            |row| {
                let package: String = row.get(0)?;
                let version: String = row.get(1)?;
                let path: String = row.get(2)?;
                let count: u64 = row.get(3)?;

                Ok((package, version, path, count))
            },
        )?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    /// Obtain global counts of all x86 instructions.
    ///
    /// Returns rows mapping the x86 instruction code to the number of occurrences
    /// in order of the total count, descending.
    pub fn x86_instruction_counts_global(&self) -> Result<Vec<(iced_x86::Code, usize)>> {
        let mut statement = self.conn.prepare_cached(indoc! {"
            SELECT instruction, SUM(occurrences) AS counts
            FROM elf_file_x86_instruction_count
            GROUP BY instruction
            ORDER BY counts DESC
        "})?;

        let res = statement.query_map([], |row| {
            let instruction: String = row.get(0)?;
            let count: usize = row.get(1)?;

            let code = X86_INSTRUCTION_CODES
                .get(&instruction)
                .expect("instruction string should be known");

            Ok((*code, count))
        })?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    /// Obtain counts of x86 instructions aggregated by package.
    ///
    /// Returns rows containing the package name, package version, x86 instruction code,
    /// and counts of instructions. Rows are ordered by package name and version in
    /// ascending order.
    pub fn x86_instruction_counts_by_package(
        &self,
    ) -> Result<Vec<(String, String, iced_x86::Code, usize)>> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT * FROM v_package_instruction_count")?;

        let res = statement.query_map([], |row| {
            let package_name: String = row.get(1)?;
            let package_version: String = row.get(2)?;
            let instruction: String = row.get(3)?;
            let count: usize = row.get(4)?;

            let code = X86_INSTRUCTION_CODES
                .get(&instruction)
                .expect("instruction value should be known");

            Ok((package_name, package_version, *code, count))
        })?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn x86_register_counts_global(&self) -> Result<Vec<(iced_x86::Register, u64)>> {
        let mut statement = self.conn.prepare_cached(indoc! {"
            SELECT register, SUM(occurrences) AS counts
            FROM elf_file_x86_register_count
            GROUP BY register
            ORDER BY counts DESC
        "})?;

        let res = statement.query_map([], |row| {
            let name: String = row.get(0)?;
            let count: u64 = row.get(1)?;

            let register = iced_x86::Register::values()
                .find(|r| format!("{:?}", r) == name)
                .expect("could not find register from name");

            Ok((register, count))
        })?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn x86_base_register_counts_global(&self) -> Result<Vec<(iced_x86::Register, u64)>> {
        let mut statement = self.conn.prepare_cached(indoc! {"
            SELECT register, SUM(occurrences) AS counts
            FROM elf_file_x86_base_register_count
            GROUP BY register
            ORDER BY counts DESC
        "})?;

        let res = statement.query_map([], |row| {
            let name: String = row.get(0)?;
            let count: u64 = row.get(1)?;

            let register = iced_x86::Register::values()
                .find(|r| format!("{:?}", r) == name)
                .expect("could not find register from name");

            Ok((register, count))
        })?;

        Ok(res.collect::<Result<Vec<_>, _>>()?)
    }

    /// Obtain CPUID features used by packages.
    ///
    /// The returned map has package names and versions as keys and a set of CPUID features
    /// as values.
    pub fn cpuid_features_by_package(&self) -> Result<HashMap<(String, String), HashSet<String>>> {
        let features_by_code = iced_x86::Code::values()
            .map(|code| {
                let features = code
                    .cpuid_features()
                    .iter()
                    .map(|f| format!("{:?}", f))
                    .collect::<HashSet<_>>();

                (code, features)
            })
            .collect::<HashMap<_, _>>();

        let mut features_by_package: HashMap<(String, String), HashSet<String>> = HashMap::new();

        for (package, version, code, _) in self.x86_instruction_counts_by_package()? {
            let key = (package, version);

            let entry = features_by_package.entry(key).or_insert_with(HashSet::new);

            for feature in features_by_code
                .get(&code)
                .expect("x86 code should be known")
            {
                entry.insert(feature.to_string());
            }
        }

        Ok(features_by_package)
    }
}

pub struct DatabaseTransaction<'txn> {
    txn: Transaction<'txn>,
}

impl<'txn> DatabaseTransaction<'txn> {
    pub fn commit(self) -> Result<()> {
        Ok(self.txn.commit()?)
    }

    /// Add a package to the database or delete and replace if it already exists.
    ///
    /// Foreign key references ensure that deletion of the original package will
    /// delete any derived data.
    pub fn add_or_replace_package(
        &self,
        name: &str,
        version: &str,
        source_url: &str,
    ) -> Result<i64> {
        let mut statement = self.txn.prepare_cached(indoc! {"
                INSERT INTO package (name, version, source_url) VALUES (?, ?, ?)
              "})?;

        match statement.execute(params![name, version, source_url]) {
            Ok(x) => Ok(x),
            Err(rusqlite::Error::SqliteFailure(err, msg)) => {
                if matches!(err.code, rusqlite::ErrorCode::ConstraintViolation) {
                    self.txn.execute(
                        "DELETE FROM package WHERE source_url = ?",
                        params![source_url],
                    )?;

                    statement.execute(params![name, version, source_url])
                } else {
                    Err(rusqlite::Error::SqliteFailure(err, msg))
                }
            }
            Err(e) => Err(e),
        }?;

        Ok(self.txn.last_insert_rowid())
    }

    /// Store an [IndexedPackage] within the database.
    ///
    /// If the package already has data stored, it will be replaced by the incoming content.
    pub fn store_indexed_package(&self, package: &IndexedPackage) -> Result<i64> {
        let package_id = self
            .add_or_replace_package(&package.name, &package.version, &package.url)
            .context("adding or replacing package")?;

        for pf in &package.files {
            let package_file_id = self
                .add_package_file(package_id, &pf.path, pf.size)
                .context("adding package file")?;

            if let Some(bi) = &pf.binary_info {
                if let Some(elf) = &bi.elf {
                    self.add_elf_file(package_file_id, elf)
                        .context("adding ELF file")?;
                }
            }
        }

        Ok(package_id)
    }

    /// Add a file belonging to a specified package.
    pub fn add_package_file(&self, package_id: i64, path: &Path, size: u64) -> Result<i64> {
        let mut statement = self.txn.prepare_cached(indoc! {"
            INSERT INTO package_file (package_id, path, size) VALUES (?, ?, ?)
        "})?;

        statement.execute(params![package_id, format!("{}", path.display()), size])?;

        Ok(self.txn.last_insert_rowid())
    }

    /// Add an ELF binary file defined by [ElfBinaryInfo].
    pub fn add_elf_file(&self, package_file_id: i64, elf: &ElfBinaryInfo) -> Result<i64> {
        let mut statement = self.txn.prepare_cached(indoc! {"
            INSERT INTO elf_file (
                package_file_id,
                class,
                data_encoding,
                os_abi,
                abi_version,
                object_file_type,
                machine,
                entry_address,
                flags,
                program_header_size,
                program_header_count,
                section_header_size,
                section_header_count,
                plt_relocations_size,
                rel_relocations_size,
                rela_relocations_size,
                string_table_size,
                init_function_address,
                termination_function_address,
                shared_object_name,
                dynamic_flags,
                dynamic_flags_1,
                runpath,
                relocations_count,
                relocations_addends_count
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "})?;

        statement.execute(params![
            package_file_id,
            elf.class,
            elf.data_encoding,
            elf.os_abi,
            elf.abi_version,
            elf.object_file_type,
            elf.machine,
            elf.entry_address,
            elf.elf_flags,
            elf.program_header_size,
            elf.program_header_len,
            elf.section_header_size,
            elf.section_header_len,
            elf.plt_relocs_size,
            elf.rel_relocs_size,
            elf.rela_relocs_size,
            elf.string_table_size,
            elf.init_function_address,
            elf.termination_function_address,
            elf.shared_object_name,
            elf.flags,
            elf.flags1,
            elf.runpath,
            elf.relocations_count,
            elf.relocations_a_count,
        ])?;

        let elf_id = self.txn.last_insert_rowid();

        self.add_elf_sections(elf_id, elf.sections.iter())
            .context("adding ELF sections")?;

        self.add_elf_file_needed_libraries(elf_id, elf.needed_libraries.iter().map(|x| x.as_str()))
            .context("adding ELF needed libraries")?;

        self.add_elf_symbols(elf_id, elf.symbols.iter())
            .context("adding ELF symbols")?;

        self.add_elf_symbols(elf_id, elf.dynamic_symbols.iter())
            .context("adding ELF dynamic symbols")?;

        self.add_elf_file_x86_instruction_counts(elf_id, &elf.instruction_counts)
            .context("adding binary file x86 instruction counts")?;

        Ok(elf_id)
    }

    pub fn add_elf_sections<'a>(
        &self,
        elf_file_id: i64,
        sections: impl Iterator<Item = &'a ElfSection>,
    ) -> Result<()> {
        let mut statement = self.txn.prepare_cached(indoc! {"
            INSERT INTO elf_section (
                elf_file_id,
                number,
                name,
                section_type,
                flags,
                address,
                offset,
                size,
                link,
                info,
                address_alignment,
                entity_size
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "})?;

        for section in sections {
            statement.execute(params![
                elf_file_id,
                section.index,
                section.name,
                section.typ,
                section.flags,
                format!("{}", section.address),
                section.offset,
                section.size,
                section.link,
                section.info,
                section.address_alignment,
                section.entity_size
            ])?;
        }

        Ok(())
    }

    pub fn add_elf_symbols<'a>(
        &self,
        elf_file_id: i64,
        symbols: impl Iterator<Item = &'a ElfSymbol>,
    ) -> Result<()> {
        let mut name_insert = self.txn.prepare_cached(indoc! {"
            INSERT INTO symbol_name (
                name,
                name_demangled
            ) VALUES (?, ?)
            RETURNING id
        "})?;

        let mut name_get = self
            .txn
            .prepare_cached("SELECT id FROM symbol_name WHERE name = ?")?;

        let mut symbol_statement = self.txn.prepare_cached(indoc! {"
            INSERT INTO elf_symbol (
                elf_file_id,
                section_index,
                symbol_index,
                name_id,
                symbol_type,
                binding,
                visibility,
                section_header_index,
                value,
                size,
                version_filename,
                version_version
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "})?;

        for symbol in symbols {
            let name_id =
                match name_insert.query_row(params![symbol.name, symbol.name_demangled], |row| {
                    let id: i64 = row.get(0)?;

                    Ok(id)
                }) {
                    Ok(id) => Ok(id),
                    Err(rusqlite::Error::SqliteFailure(err, msg)) => {
                        if matches!(err.code, rusqlite::ErrorCode::ConstraintViolation) {
                            name_get.query_row(params![symbol.name], |row| {
                                let id: i64 = row.get(0)?;

                                Ok(id)
                            })
                        } else {
                            Err(rusqlite::Error::SqliteFailure(err, msg))
                        }
                    }
                    Err(e) => Err(e),
                }?;

            symbol_statement.execute(params![
                elf_file_id,
                symbol.section_index,
                symbol.symbol_index,
                name_id,
                symbol.typ,
                symbol.bind,
                symbol.visibility,
                symbol.section_header_index,
                format!("{}", symbol.value),
                format!("{}", symbol.size),
                symbol.version_file,
                symbol.version_version,
            ])?;
        }

        Ok(())
    }

    /// Define required libraries for a given binary file.
    pub fn add_elf_file_needed_libraries<'a>(
        &self,
        elf_file_id: i64,
        values: impl Iterator<Item = &'a str>,
    ) -> Result<()> {
        let mut statement = self.txn.prepare_cached(indoc! {"
            INSERT INTO elf_file_needed_library (elf_file_id, name) VALUES (?, ?)
        "})?;

        for name in values {
            statement.execute(params![elf_file_id, name])?;
        }

        Ok(())
    }

    /// Annotate x86 instruction counts for a binary file.
    pub fn add_elf_file_x86_instruction_counts(
        &self,
        elf_file_id: i64,
        counts: &X86InstructionCounts,
    ) -> Result<()> {
        let mut statement = self.txn.prepare_cached(indoc! {"
            INSERT INTO elf_file_x86_instruction_count (elf_file_id, instruction, occurrences)
            VALUES (?, ?, ?)
        "})?;

        for (code, count) in counts.code_counts() {
            statement.execute(params![
                elf_file_id,
                format!("{:?}", code).to_lowercase(),
                count
            ])?;
        }

        let mut statement = self.txn.prepare_cached(indoc! {"
            INSERT INTO elf_file_x86_register_count (elf_file_id, register, occurrences)
            VALUES (?, ?, ?)
        "})?;

        for (register, count) in counts.register_counts() {
            statement.execute(params![elf_file_id, format!("{:?}", register), count])?;
        }

        let mut statement = self.txn.prepare_cached(indoc! {"
            INSERT INTO elf_file_x86_base_register_count (elf_file_id, register, occurrences)
            VALUES (?, ?, ?)
        "})?;

        for (register, count) in counts.base_register_counts() {
            statement.execute(params![elf_file_id, format!("{:?}", register), count])?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {

    use {
        super::*, debian_packaging::repository::builder::DebPackageReference,
        futures_util::AsyncReadExt,
    };

    #[tokio::test]
    async fn high_level() -> Result<()> {
        let mut db = DatabaseConnection::new_memory()?;

        let client =
            debian_packaging::repository::reader_from_str("http://us.archive.ubuntu.com/ubuntu")?;
        let release = client.release_reader("jammy").await?;

        let libc = release
            .resolve_packages("main", "amd64", false)
            .await?
            .into_iter()
            .find(|package| matches!(package.package(), Ok("libc6")))
            .expect("libc6 package should be found");
        let libc_path = libc.required_field_str("Filename")?.to_string();

        let mut reader = client
            .fetch_binary_package_generic(debian_packaging::repository::BinaryPackageFetch {
                control_file: libc.clone(),
                path: libc_path.clone(),
                size: libc.size().expect("Size should be defined")?,
                digest: libc
                    .deb_digest(debian_packaging::repository::release::ChecksumType::Sha256)?,
            })
            .await?;

        let mut data = vec![];
        reader.read_to_end(&mut data).await?;

        crate::import::import_debian_package_from_data(
            client.url()?.join(&libc_path)?.as_str(),
            data,
            &mut db,
        )
        .await
        .context("importing debian package")?;

        let urls = db.package_urls().context("package_urls")?;
        assert_eq!(urls.len(), 1);

        assert_eq!(db.elf_file_count()?, 274);

        let counts = db.elf_file_section_counts_global()?;
        assert_eq!(counts.len(), 79);
        assert_eq!(counts[0], ("".to_string(), 274));
        assert_eq!(counts[1], (".bss".to_string(), 274));

        let ifuncs = db.elf_file_ifuncs()?;

        let values = ifuncs
            .iter()
            .find(|((_, _, path), _)| path == "lib/x86_64-linux-gnu/libc.so.6")
            .expect("should find ifuncs for libc.so.6")
            .1;
        assert!(values.contains("memcmp"));

        let importing = db
            .elf_files_importing_symbol("malloc")
            .context("elf_files_importing_symbol")?;
        assert_eq!(importing.len(), 9);
        assert_eq!(importing[0].0, "libc6");
        assert_eq!(importing[0].2, "lib/x86_64-linux-gnu/libnsl.so.1");

        db.elf_files_defining_symbol("memcpy")
            .context("file_files_defining_symbol")?;
        db.x86_instruction_counts_by_binary(None)
            .context("x86_instruction_counts_by_binary")?;
        db.x86_instruction_counts_by_binary(Some("add_rm8_r8"))
            .context("x86_instruction_counts_by_binary with arg")?;
        db.x86_instruction_counts_global()
            .context("x86_instruction_counts_global")?;
        db.x86_instruction_counts_by_package()
            .context("x86_instruction_counts_by_package")?;
        db.x86_register_counts_global()
            .context("x86_register_counts_global")?;
        db.x86_base_register_counts_global()
            .context("x86_base_register_counts_global")?;
        db.cpuid_features_by_package()
            .context("cpuid_features_by_package")?;

        Ok(())
    }
}
