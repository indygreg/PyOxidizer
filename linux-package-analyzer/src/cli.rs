// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    clap::{Arg, ArgMatches, Command},
    std::collections::{HashMap, HashSet},
};

const ABOUT: &str = "\
# About

`lpa` provides a mechanism for analyzing the contents of Linux packages.

`lpa` works by importing a source of Linux packages (e.g. a Debian or RPM
repository) and storing the indexed result in a local SQLite database. The
SQLite database can then be queried by `lpa` (or your own code if desired)
to answer questions about content therein.
";

const IMPORT_DEBIAN_REPOSITORY_ABOUT: &str = "\
Imports the contents of a Debian repository.

This command will take an HTTP hosted Debian repository, discover all its
packages, then proceed to download and index discovered packages.

The provided URL is the directory containing the `InRelease` file. Example
values include:

* http://ftp.us.debian.org/debian (Debian)
* http://us.archive.ubuntu.com/ubuntu (Ubuntu)
";

pub async fn run() -> Result<()> {
    let default_threads = format!("{}", num_cpus::get());

    let app = Command::new("Linux Package Analyzer")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .about("Analyze the content of Linux packages")
        .long_about(ABOUT)
        .arg_required_else_help(true);

    let app = app.arg(
        Arg::new("db_path")
            .long("--db")
            .default_value("lpa.db")
            .takes_value(true)
            .global(true)
            .help("Path to SQLite database to use"),
    );

    let app = app.arg(
        Arg::new("threads")
            .short('t')
            .long("threads")
            .takes_value(true)
            .default_value(&default_threads)
            .global(true)
            .help("Number of threads to use"),
    );

    let app = app.subcommand(
        Command::new("import-debian-deb")
            .about("Import a Debian .deb package given a filesystem path")
            .arg(
                Arg::new("path")
                    .required(true)
                    .help("Path to .deb file to import"),
            ),
    );

    let app = app.subcommand(
        Command::new("import-debian-repository")
            .about("Import the contents of a Debian repository")
            .long_about(IMPORT_DEBIAN_REPOSITORY_ABOUT)
            .arg(
                Arg::new("architectures")
                    .long("architectures")
                    .takes_value(true)
                    .default_value("amd64")
                    .help("Comma delimited list of architectures to fetch"),
            )
            .arg(
                Arg::new("components")
                    .long("components")
                    .takes_value(true)
                    .default_value("main")
                    .help("Comma delimited list of components to fetch"),
            )
            .arg(
                Arg::new("url")
                    .required(true)
                    .help("Base URL of Debian repository to import"),
            )
            .arg(
                Arg::new("distribution")
                    .required(true)
                    .help("Distribution to import"),
            ),
    );

    let app = app.subcommand(
        Command::new("import-rpm-repository")
            .about("Import the contents of an RPM repository")
            .arg(
                Arg::new("url")
                    .required(true)
                    .help("Base URL of RPM repository to import"),
            ),
    );

    let app = app.subcommand(
        Command::new("cpuid-features-by-package-count")
            .about("Print CPUID features and counts of packages having instructions with them"),
    );

    let app = app.subcommand(Command::new("elf-files").about("Print known ELF files"));

    let app = app.subcommand(
        Command::new("elf-files-defining-symbol")
            .about("Print ELF files defining a named symbol")
            .arg(
                Arg::new("symbol")
                    .takes_value(true)
                    .required(true)
                    .help("Name of symbol to search for"),
            ),
    );

    let app = app.subcommand(
        Command::new("elf-files-importing-symbol")
            .about("Print ELF files importing a specified named symbol")
            .arg(
                Arg::new("symbol")
                    .takes_value(true)
                    .help("Symbol name to match against"),
            ),
    );

    let app = app.subcommand(
        Command::new("elf-files-with-ifunc")
            .about("Print ELF files that leverage IFUNC for dynamic dispatch"),
    );

    let app = app.subcommand(
        Command::new("elf-file-total-x86-instruction-counts")
            .about("Print the total number of instructions in all ELF files")
            .arg(
                Arg::new("instruction")
                    .long("--instruction")
                    .takes_value(true)
                    .help("Name of instruction to count"),
            ),
    );

    let app = app.subcommand(
        Command::new("elf-section-name-counts").about("Print counts of section names in ELF files"),
    );

    let app = app.subcommand(
        Command::new("packages-with-cpuid-feature")
            .about("Print packages having instructions with a given CPUID feature")
            .arg(
                Arg::new("feature")
                    .takes_value(true)
                    .multiple_values(true)
                    .required(true)
                    .help("Name of CPUID feature to filter on"),
            ),
    );

    let app = app.subcommand(
        Command::new("packages-with-filename")
            .about("Print packages having a file with the specified name")
            .arg(
                Arg::new("filename")
                    .takes_value(true)
                    .required(true)
                    .help("Exact name of file to match against"),
            ),
    );

    let app = app.subcommand(
        Command::new("x86-instruction-counts").about("Print global counts of x86 instructions"),
    );

    let app = app.subcommand(
        Command::new("x86-register-usage-counts")
            .about("Print counts of how many x86 instructions use known registers")
            .arg(
                Arg::new("base")
                    .long("--base")
                    .help("Normalize to base register name"),
            ),
    );

    let app = app.subcommand(
        Command::new("reference-x86-cpuid-features")
            .about("Print a list of known x86 CPUID features"),
    );

    let app = app.subcommand(
        Command::new("reference-x86-instructions").about("Print a list of known x86 instructions"),
    );

    let app = app.subcommand(
        Command::new("reference-x86-registers").about("Print a list of known x86 registers"),
    );

    let matches = app.get_matches();

    let (command, args) = matches
        .subcommand()
        .ok_or_else(|| anyhow!("invalid sub-command"))?;

    match command {
        "import-debian-deb" => command_import_debian_deb(args).await,
        "import-debian-repository" => command_import_debian_repository(args).await,
        "import-rpm-repository" => command_import_rpm_repository(args).await,
        "cpuid-features-by-package-count" => command_cpuid_features_by_package_count(args),
        "elf-files" => command_elf_files(args),
        "elf-files-defining-symbol" => command_elf_files_defining_symbol(args),
        "elf-files-with-ifunc" => elf_files_with_ifunc(args),
        "elf-files-importing-symbol" => command_elf_files_importing_symbol(args),
        "elf-file-total-x86-instruction-counts" => {
            command_elf_file_total_x86_instruction_counts(args)
        }
        "elf-section-name-counts" => command_elf_section_name_counts(args),
        "packages-with-cpuid-feature" => command_packages_with_cpuid_feature(args),
        "packages-with-filename" => command_packages_with_filename(args),

        "x86-instruction-counts" => command_x86_instruction_counts(args),
        "x86-register-usage-counts" => command_x86_register_usage_counts(args),

        "reference-x86-cpuid-features" => command_reference_cpuid_features(),
        "reference-x86-instructions" => command_reference_x86_instructions(),
        "reference-x86-registers" => command_reference_x86_registers(),
        _ => panic!("unhandled sub-command"),
    }
}

async fn command_import_debian_deb(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");
    let path = args.value_of("path").expect("path argument is required");

    let mut db = crate::db::DatabaseConnection::new_path(db_path)?;

    let data = std::fs::read(path)?;

    let url = url::Url::from_file_path(path)
        .map_err(|e| anyhow!("failed to resolve package URL: {:?}", e))?;

    crate::import::import_debian_package_from_data(url.as_str(), data, &mut db).await?;

    Ok(())
}

async fn command_import_debian_repository(args: &ArgMatches) -> Result<()> {
    let threads = args.value_of_t::<usize>("threads")?;
    let db_path = args.value_of("db_path").expect("database path is required");
    let url = args.value_of("url").expect("url argument is required");
    let distribution = args
        .value_of("distribution")
        .expect("distribution argument is required");

    let architectures = args
        .value_of("architectures")
        .expect("architectures argument is required")
        .split(',')
        .map(|x| x.to_string())
        .collect::<Vec<_>>();
    let components = args
        .value_of("components")
        .expect("components argument is required")
        .split(',')
        .map(|x| x.to_string())
        .collect::<Vec<_>>();

    let mut db = crate::db::DatabaseConnection::new_path(db_path)?;

    let root_reader = debian_packaging::repository::reader_from_str(url)?;
    eprintln!("fetching InRelease file for {}", distribution);
    let release = root_reader.release_reader(distribution).await?;

    let fetches = release
        .resolve_package_fetches(
            Box::new(move |entry| {
                if !entry.is_installer
                    && architectures.contains(&entry.architecture.to_string())
                    && components.contains(&entry.component.to_string())
                {
                    eprintln!(
                        "fetching packages from {} {}",
                        entry.component, entry.architecture
                    );
                    true
                } else {
                    eprintln!(
                        "ignoring {} packages from {} {}",
                        if entry.is_installer {
                            "installer"
                        } else {
                            "non-installer"
                        },
                        entry.component,
                        entry.architecture
                    );
                    false
                }
            }),
            Box::new(|_| true),
            threads,
        )
        .await?;

    eprintln!("resolved {} packages to import", fetches.len());

    crate::import::import_debian_packages(
        root_reader.as_ref(),
        fetches.into_iter(),
        &mut db,
        threads,
    )
    .await?;

    Ok(())
}

async fn command_import_rpm_repository(_: &ArgMatches) -> Result<()> {
    eprintln!("RPM functionality has been disabled because the rpm-rs crate is not maintained.");
    eprintln!("See https://github.com/indygreg/PyOxidizer/issues/619 for more");
    Err(anyhow!("functionality disabled"))

    /*
    let threads = args.value_of_t::<usize>("threads")?;
    let db_path = args.value_of("db_path").expect("database path is required");
    let url = args.value_of("url").expect("url argument is required");

    let mut db = crate::db::DatabaseConnection::new_path(db_path)?;

    let root_reader = rpm_repository::http::HttpRepositoryClient::new(url)?;
    eprintln!("fetching repo metadata");
    let metadata = root_reader.metadata_reader().await?;

    eprintln!("fetching primary packages");
    let primary_packages = metadata.primary_packages().await?;
    eprintln!("resolved {} packages", primary_packages.count);

    crate::import::import_rpm_packages(
        &root_reader,
        primary_packages.packages.into_iter(),
        &mut db,
        threads,
    )
    .await?;

    Ok(())

         */
}

fn command_elf_files(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");

    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    for (package, version, path) in db.elf_files()? {
        println!("{} {} {}", package, version, path);
    }

    Ok(())
}

fn command_elf_files_defining_symbol(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");
    let symbol = args
        .value_of("symbol")
        .expect("symbol argument is required");

    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    for (package, version, path) in db.elf_files_defining_symbol(symbol)? {
        println!("{} {} {}", package, version, path);
    }

    Ok(())
}

fn command_packages_with_filename(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");
    let filename = args
        .value_of("filename")
        .expect("filename argument is required");

    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    for (package, version, path) in db.packages_with_filename(filename)? {
        println!("{} {} {}", package, version, path);
    }

    Ok(())
}

fn command_elf_file_total_x86_instruction_counts(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");
    let instruction = args.value_of("instruction");

    if let Some(instruction) = instruction {
        iced_x86::Code::values()
            .find(|code| format!("{:?}", code).to_lowercase() == instruction.to_lowercase())
            .ok_or_else(|| anyhow!("failed to find op code for instruction: {}", instruction))?;
    }

    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    let counts = db.x86_instruction_counts_by_binary(instruction)?;

    for (package, version, path, count) in counts {
        println!("{:>12}\t{}\t{}\t{}", count, package, version, path);
    }

    Ok(())
}

fn command_elf_section_name_counts(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");
    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    let counts = db.elf_file_section_counts_global()?;

    for (section, count) in counts {
        println!("{:>8}\t{}", count, section);
    }

    Ok(())
}

fn command_cpuid_features_by_package_count(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");
    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    let features_by_package = db.cpuid_features_by_package()?;

    let mut feature_counts: HashMap<String, usize> = HashMap::new();

    for package_features in features_by_package.values() {
        for feature in package_features {
            let entry = feature_counts.entry(feature.to_string()).or_default();
            *entry += 1;
        }
    }

    let mut feature_counts = feature_counts.into_iter().collect::<Vec<_>>();
    feature_counts.sort_by(|(_, a), (_, b)| b.cmp(a));

    println!("{:>10}\t{:>20}\tPercentage", "Packages", "CPUID Feature",);

    println!(
        "{:>10}\t{:>20}\t{:.2}%",
        features_by_package.len(),
        "Any Feature",
        100.0
    );
    for (feature, count) in feature_counts {
        let percentage = (count as f32 / features_by_package.len() as f32) * 100.0;

        println!("{:>10}\t{:>20}\t{:.2}%", count, feature, percentage);
    }

    Ok(())
}

fn elf_files_with_ifunc(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");

    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    for ((package, version, path), symbols) in db.elf_file_ifuncs()? {
        let symbols = symbols.into_iter().collect::<Vec<_>>();
        println!("{}:{}:{}\t{}", package, version, path, symbols.join(", "));
    }

    Ok(())
}

fn command_elf_files_importing_symbol(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");
    let symbol = args
        .value_of("symbol")
        .expect("symbol argument is required");

    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    for (package_name, package_version, path) in db.elf_files_importing_symbol(symbol)? {
        println!("{} {} {}", package_name, package_version, path);
    }

    Ok(())
}

fn command_packages_with_cpuid_feature(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");
    let wanted_features = args
        .values_of("feature")
        .expect("feature argument is required")
        .map(|x| x.to_string())
        .collect::<HashSet<_>>();

    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    let mut features_by_package = db
        .cpuid_features_by_package()?
        .into_iter()
        .collect::<Vec<_>>();
    features_by_package.sort_by(|(a_key, _), (b_key, _)| a_key.cmp(b_key));

    for ((name, version), features) in features_by_package {
        if features.intersection(&wanted_features).count() != 0 {
            println!("{} {}", name, version);
        }
    }

    Ok(())
}

fn command_x86_instruction_counts(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");

    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    for (code, count) in db.x86_instruction_counts_global()? {
        println!("{:>12}\t{}", count, code.op_code().instruction_string());
    }

    Ok(())
}

fn command_x86_register_usage_counts(args: &ArgMatches) -> Result<()> {
    let db_path = args.value_of("db_path").expect("database path is required");
    let base = args.is_present("base");

    let db = crate::db::DatabaseConnection::new_path(db_path)?;

    let counts = if base {
        db.x86_base_register_counts_global()?
    } else {
        db.x86_register_counts_global()?
    };

    for (register, count) in counts {
        println!("{:>12}\t{:?}", count, register);
    }

    Ok(())
}

fn command_reference_cpuid_features() -> Result<()> {
    for feature in iced_x86::CpuidFeature::values() {
        println!("{:?}", feature);
    }

    Ok(())
}

fn command_reference_x86_instructions() -> Result<()> {
    for code in iced_x86::Code::values() {
        println!(
            "{}\t{}",
            format!("{:?}", code).to_lowercase(),
            code.op_code().instruction_string()
        );
    }

    Ok(())
}

fn command_reference_x86_registers() -> Result<()> {
    for register in iced_x86::Register::values() {
        println!("{:?}", register);
    }

    Ok(())
}
