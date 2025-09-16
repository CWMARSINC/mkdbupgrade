/*
 * Copyright © 2025 C/W MARS, Inc.
 * Author: Jason Stephenson <jason@sigio.com>
 *
 * This file is part of mkdbupgrade.
 *
 * mkdbupgrade is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 2 of the License, or
 * (at your option) any later version.
 *
 * mkdbupgrade is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with mkdbupgrade.  If not, see <http://www.gnu.org/licenses/>.
 */
use clap::Parser;
use regex::Regex;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;
use mkdbupgrade::*;

#[derive(Parser, Debug)]
#[command(about, long_about)]
/// Make a custom database upgrade script from one version of Evergreen to another.
pub struct Cli {
    /// Evergreen git branch we are upgrading from
    #[arg(short,long)]
    from_branch: String,
    /// Evergreen version we are upgrading from. Calculated from previous branch name if absent.
    #[arg(short='F',long)]
    from_version: Option<String>,
    /// Version of Evergreen we are upgrading to. Calculated from current branch name if absent. An error occurs if it cannot be calculated.
    #[arg(short,long)]
    version: Option<String>,
    /// Database upgrade(s) to move to after the main transaction. May be repeated to move additional upgrades.
    #[arg(short, long="move")]
    moved: Option<Vec<String>>,
    /// File to append to end of output upgrade script. May be repeated to add additional files.
    #[arg(short,long)]
    append_file: Option<Vec<String>>,
    /// File to prepend to end of output upgrade script. May be repeated to add additional files.
    #[arg(short,long)]
    prepend_file: Option<Vec<String>>,
    /// Output directory where to write the database upgrade script file.
    #[arg(short='O',long, default_value="Open-ILS/src/sql/Pg/version-upgrade")]
    output_directory: String,
    /// Prefix to add to output file name.
    #[arg(short='P',long)]
    prefix: Option<String>,
    /// Overwrite an existing output file with the same name. Otherwise an error is signaled if a file of the same name exists.
    #[arg(short='C',long)]
    clobber: bool,
    /// Review or edit the result in your EDITOR.
    #[arg(short,long)]
    review: bool,
}

fn main() {
    let cli = Cli::parse();

    // Assumes we're in the Evergreen git repository with the correct
    // branch checked out. This also makes a quick test if we're in a
    // git repository.
    let repository = match get_repository() {
        Some(r) => r,
        None => {
            eprintln!("Current directory is not a git repository");
            exit(1);
        }
    };
    let to_branch = match get_current_branch(&repository) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            exit(1);
        },
    };

    let to_branch_name = match to_branch.name() {
        Ok(Some(s)) => s,
        Ok(None) => "unknown branch",
        Err(e) => {
            eprintln!("{e}");
            exit(1);
        }
    };

    // Check for the Open-ILS subdirectory as an extra precaution.
    let checkdir = Path::new("Open-ILS");
    if ! checkdir.exists() || ! checkdir.is_dir() {
        eprintln!("Not in an Evergreen repository, exiting");
        exit(1);
    }

    // The "from" or source branch is required, so let's check if it
    // exists.
    let from_branch = match find_branch(&repository, &cli.from_branch) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error finding from branch {}: {}", &cli.from_branch, e);
            exit(1);
        }
    };

    // The version of Evergreen that we're upgrading to.
    let version = match cli.version {
        Some(v) => v,
        None => {
            match get_branch_version(&to_branch) {
                Some(v) => v,
                None => {
                    eprintln!("Unable to determine version from branch: {}",
                              to_branch_name);
                    eprintln!("Specify the new Evergreen version with -v [version]");
                    exit(1);
                }
            }
        },
    };

    // The version of Evergreen that we're upgrading from.
    let from_version = match cli.from_version {
        Some(v) => v,
        None => {
            match get_branch_version(&from_branch) {
                Some(v) => v,
                None => {
                    eprintln!("Unable to determine version from branch: {}",
                              &cli.from_branch);
                    eprintln!("Specify the old Evergreen version with -F [version]");
                    exit(1);
                }
            }
        },
    };

    // Filename for the database upgrade script.
    let upgrade_filename = match cli.prefix {
        Some(p) => format!("{}{}-{}-upgrade-db.sql", p, from_version, version),
        None => format!("{}-{}-upgrade-db.sql", from_version, version),
    };
    // We're going to use out_path for opening and writing the file.
    let mut out_path = PathBuf::new();
    out_path.push(cli.output_directory);
    out_path.push(upgrade_filename);
    if out_path.exists() && ! cli.clobber {
        eprintln!("Output file {} exists, exiting", out_path.display());
        eprintln!("You can overwrite it the the -C option");
        exit(1);
    }

    // Preliminaries out of the way, get the list of new upgrades.
    let upgrades: Vec<String> = match get_upgrades(&repository, &from_branch, &to_branch) {
        Ok(vec) => vec,
        Err(e) => {
            eprintln!("{e}");
            exit(1);
        }
    };

    // Should we bail if upgrades.len() is 0?
    if upgrades.len() == 0 {
        eprintln!("No upgrades were found. Nothing to do.");
        exit(1);
    }

    // Create the output file and begin doing the real work.
    let mut outfile = match File::create(&out_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}");
            exit(1);
        },
    };

    match cli.prepend_file {
        Some(v) => {
            writeln!(&mut outfile, "-- Start of prepended code").expect("Unable to write to output");
            for file in v {
                match write_file(&mut outfile, &file) {
                    Ok(_) => (),
                    Err(e) => {
                        eprintln!("Error prepending file {}: {}", &file, e);
                        exit(1);
                    }
                }
            }
            writeln!(&mut outfile, "-- End of prepended code\n").expect("Unable to write to output");
        },
        None => (),
    }

    // Write our preamble.
    writeln!(&mut outfile, "-- Upgrade script for Evergreen {from_version} to {version}")
        .expect("Unable to write to output");
    writeln!(&mut outfile, "\\set eg_version '''{version}'''").expect("Unable to write to output");
    writeln!(&mut outfile, "\nBEGIN;").expect("Unable to write to output");

    // Set up to handle upgrades that need to be moved.
    let movedre: Option<Regex> = match cli.moved {
        Some(v) => {
            let mut restr = String::from("(?:");
            let mut add_pipe = false;
            for upgrade in v {
                if add_pipe {
                    restr.push('|');
                }
                restr.push_str(&upgrade);
                add_pipe = true;
            }
            restr.push(')');
            Some(Regex::new(&restr).unwrap())
        },
        None => None,
    };
    let mut moved: Vec<String> = Vec::new();

    for file in upgrades {
        let mut skip = false;
        match movedre {
            Some(ref re) => {
                if re.is_match(&file) {
                    moved.push(file.clone());
                    skip = true;
                }
            },
            None => (),
        }
        if ! skip {
            match write_upgrade(&mut outfile, &file) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Error writing upgrade {}: {}", &file, e);
                    exit(1);
                }
            }
        }
    }
    writeln!(&mut outfile, "COMMIT;\n").expect("Unable to write to output");
    if moved.len() > 0 {
        writeln!(&mut outfile, "-- Start of moved upgrades").expect("Unable to write to output");
        for file in moved {
            match write_file(&mut outfile, &file) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Error writing moved upgrade {}: {}", &file, e);
                    exit(1);
                }
            }
        }
        writeln!(&mut outfile, "-- End of moved upgrades\n").expect("Unable to write to output");
    }

    // Write code to update the auditor tables
    writeln!(&mut outfile, "-- Update auditor tables to catch changes in source tables.").expect("Unable to write to output");
    writeln!(&mut outfile, "-- Can be removed/skipped if there were no schema changes.").expect("Unable to write to output");
    writeln!(&mut outfile, "SELECT auditor.update_auditors();").expect("Unable to write to output");

    match cli.append_file {
        Some(v) => {
            writeln!(&mut outfile, "\n-- Start of appended code").expect("Unable to write to output");
            for file in v {
                match write_file(&mut outfile, &file) {
                    Ok(_) => (),
                    Err(e) => {
                        eprintln!("Error appending file {}: {}", &file, e);
                        exit(1);
                    }
                }
            }
            writeln!(&mut outfile, "-- End of appended code").expect("Unable to write to output");
        },
        None => (),
    }

    // Make sure that the output is written before we might open it in
    // the editor.
    drop(outfile);

    if cli.review {
        match review_file(&out_path.display().to_string()) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("{e}");
                exit(1);
            },
        }
    }
}
