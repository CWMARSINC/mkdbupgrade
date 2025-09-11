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
use std::error::Error;
use std::process::{Command, Stdio};
use std::fs::{File, read_to_string};
use std::io::prelude::*;
use regex::Regex;

/// Get the current git branch name
///
/// Use /usr/bin/git to the get the current branch
///
/// Returns an error if git is not installed; git fails for any reason,
/// or if Rust cannot convert the output from git to UTF-8.
pub fn get_current_branch () -> Result<String, Box<dyn Error>> {
    let output = Command::new("/usr/bin/git").arg("rev-parse")
        .arg("--abbrev-ref").arg("HEAD").stdout(Stdio::piped())
        .stderr(Stdio::piped()).output()?;
    if output.status.success() {
        let data = match String::from_utf8(output.stdout) {
            Ok(s) => s.trim_end().to_string(),
            Err(e) => return Err(Box::new(e)),
        };
        Ok(data)
    } else {
        let stderr = String::from_utf8(output.stderr).unwrap().trim_end().to_string();
        Err(stderr.into())
    }
}

/// Verify that a branch is visible in the currentl repository
///
/// Uses /usr/bin/git to verify the existence of a git branch.
/// The branch may be local or remote.
///
/// Returns true if the branch exists, false if not.
/// Return an error if the execuiton of /usr/bin/git fails.
pub fn verify_branch(branch: &String) -> Result<bool, Box<dyn Error>>  {
    let status = match Command::new("/usr/bin/git").arg("rev-parse")
        .arg("--quiet").arg("--verify").arg(branch).stdout(Stdio::null())
        .status() {
            Ok(s) => s,
            Err(e) => return Err(Box::new(e)),
        };
    Ok(status.success())
}

/// Get the "version" from a git branch name
///
/// Looks for a string like _X_Y_Z (where X, Y, an Z are 1 or two-digit
/// numbers) in the string passed as an argument. This string is
/// assumed to be a git branch name.
///
/// If the pattern is matched, returns an Option with a string value
/// of X.Y.Z. If not, None is returned.
pub fn get_branch_version(branch: &String) -> Option<String> {
    // Assumes a branch named like rel_X_Y_Z.
    let regex = Regex::new(r"_(\d{1,2})_(\d{1,2})_(\d{1,2})").unwrap();
    let Some((_, [x, y, z])) =
        regex.captures(branch).map(|caps| caps.extract()) else { return None };
    Some(format!("{}.{}.{}", x, y, z))
}

/// Get a list of Evergreen database upgrade files from a given branch
///
/// This private function is undocumened, but has similar return
/// values to other functions that use /usr/bin/git.
fn get_branch_upgrades(branch: &String) -> Result<Vec<String>, Box<dyn Error>> {
    let output = Command::new("/usr/bin/git").arg("ls-tree").arg("--name-only")
        .arg(branch).arg("--").arg("Open-ILS/src/sql/Pg/upgrade/")
        .stdout(Stdio::piped()).stderr(Stdio::piped()).output()?;
    if output.status.success() {
        match String::from_utf8(output.stdout) {
            Ok(input) => {
                let mut data: Vec<String> = Vec::new();
                for line in input.split_terminator("\n").collect::<Vec<&str>>() {
                    data.push(String::from(line));
                }
                Ok(data)
            },
            Err(e) => return Err(Box::new(e)),
        }
    } else {
        let stderr = String::from_utf8(output.stderr).unwrap().trim_end().to_string();
        Err(stderr.into())
    }
}

/// Get the list of ugprades needed to upgrade from "from" to "to" branches
///
/// Uses the private get_branch_upgrades funtion, which uses
/// /usr/bin/git, to get the upgrades in the "from" and "to"
/// branches.
///
/// Returns a vector of Strings with the upgrades in the "to" branch
/// that do not exist in the "from" branch on success. Returns any
/// errors that occur on failure.
pub fn get_upgrades(from: &String, to: &String) -> Result<Vec<String>, Box<dyn Error>> {
    let from_upgrades: Vec<String> = get_branch_upgrades(from)?;
    let to_upgrades: Vec<String> = get_branch_upgrades(to)?;
    let upgrades: Vec<String> = to_upgrades.into_iter().filter(|item| !from_upgrades.contains(item)).collect();
    Ok(upgrades)
}

/// Read a file and write its contents to the output file
///
/// Read a file (inf) and write its entire contents to the output file
/// handle (outf).
///
/// Returns any errors that occur or an empty result on success.
pub fn write_file(mut outf: &File, inf: &String) -> Result<(), Box<dyn Error>> {
    match read_to_string(inf) {
        Ok(lines) => match outf.write_all(lines.as_bytes()) {
            Ok(_) => (),
            Err(e) => return Err(Box::new(e)),
        },
        Err(e) => return Err(Box::new(e)),
    }
    Ok(())
}

/// Read an upgrade file and write its contents to the output file
///
/// Read the upgrade file (inf) and write its contents, minus the
/// "BEGIN;" and "COMMIT;" lines, to the output file handle (outf).
///
/// Returns any errors that occur or an empty Result on success.
pub fn write_upgrade(mut outf: &File, inf: &String) -> Result<(), Box<dyn Error>> {
    match read_to_string(inf) {
        Ok(lines) => {
            let re = Regex::new("^(?:BEGIN|COMMIT);").unwrap();
            for line in lines.split_terminator("\n").collect::<Vec<&str>>() {
                if ! re.is_match(line) {
                    match writeln!(outf, "{}", line) {
                        Ok(_) => (),
                        Err(e) => return Err(Box::new(e)),
                    }
                }
            }
        },
        Err(e) => return Err(Box::new(e)),
    }
    Ok(())
}

/// Open the output file in the user's EDITOR for review
///
/// Opens the output file in the program set in the user's EDITOR
/// environment variable.
///
/// Returns any errors that occur, usually if the EDITOR variable is
/// not set, or the editor cannot be run.
///
/// Return an empty result on success.
pub fn review_file(file: &String) -> Result<(), Box<dyn Error>> {
    let editor = match std::env::var("EDITOR") {
        Ok(ed) => ed,
        Err(e) => return Err(Box::new(e)),
    };
    let args: Vec<&str> = editor.split_whitespace().collect();
    let mut cmd = Command::new(args[0]);
    for arg in &args[1..] {
        cmd.arg(arg);
    }
    cmd.arg(file);
    match cmd.spawn() {
        Ok(_) => Ok(()),
        Err(e) => Err(Box::new(e)),
    }
}
