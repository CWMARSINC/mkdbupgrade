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
use git2::{Branch, BranchType, ObjectType, Repository, TreeWalkMode, TreeWalkResult};
use regex::Regex;
use std::env::var;
use std::error::Error;
use std::fmt;
use std::fs::{File, read_to_string};
use std::io::prelude::*;
use std::path::Path;
use std::process::Command;

/// Error returned if current repository head reference is not a branch
#[derive(Debug, Clone)]
pub struct HeadError;

impl fmt::Display for HeadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "head is not a branch")
    }
}

impl Error for HeadError {}

/// Get reference to current git repository
///
/// Returns None if current directory is not a repository
pub fn get_repository() -> Option<Repository> {
    match Repository::open("./") {
        Ok(r) => Some(r),
        Err(_) => None,
    }
}

/// Get the current git branch name in repository
///
pub fn get_current_branch(repo: &Repository) -> Result<Branch<'_>, Box<dyn Error>> {
    let head = match repo.head() {
        Ok(h) => h,
        Err(e) => return Err(Box::new(e)),
    };
    if head.is_branch() {
        Ok(Branch::wrap(head))
    } else {
        Err(HeadError.into())
    }
}

/// Find named branch in the repository
///
/// Searches for local and remote branches. Returns the branch object
/// if found.
pub fn find_branch<'a>(repo: &'a Repository, name: &String) -> Result<Branch<'a>, Box<dyn Error>> {
    match repo.find_branch(name, BranchType::Local) {
        Ok(b) => Ok(b),
        Err(_) => {
            match repo.find_branch(name, BranchType::Remote) {
                Ok(r) => Ok(r),
                Err(e) => Err(Box::new(e)),
            }
        },
    }
}


/// Get the "version" from a git branch name
///
/// Looks for a string like _X_Y_Z (where X, Y, an Z are 1 or two-digit
/// numbers) in the name of the branch passed as an argument.
///
/// If the pattern is matched, returns an Option with a string value
/// of X.Y.Z. If not, None is returned.
pub fn get_branch_version(branch: &Branch) -> Option<String> {
    // Assumes a branch named like rel_X_Y_Z.
    let regex = Regex::new(r"_(\d{1,2})_(\d{1,2})_(\d{1,2})").unwrap();
    let branch_name = match branch.name() {
        Ok(Some(s)) => s,
        Ok(None) => return None,
        Err(_) => return None,
    };
    let Some((_, [x, y, z])) =
        regex.captures(branch_name).map(|caps| caps.extract()) else { return None };
    Some(format!("{}.{}.{}", x, y, z))
}

/// Get a list of Evergreen database upgrade files from a given branch
fn get_branch_upgrades(repo: &Repository, branch: &Branch) -> Result<Vec<String>, Box<dyn Error>> {
    let mut upgrades: Vec<String> = Vec::new();
    let dirpath = "Open-ILS/src/sql/Pg/upgrade";
    let tree = branch.get().peel_to_tree()?;
    match tree.get_path(Path::new(dirpath)) {
        Ok(tree_entry) => {
            if let Some(ObjectType::Tree) = tree_entry.kind() {
                let object = tree_entry.to_object(&repo)?;
                let dir_tree = object.as_tree().unwrap();
                dir_tree.walk(TreeWalkMode::PreOrder, |_, entry| {
                    match entry.name() {
                        Some(n) => upgrades.push(format!("{}/{}", dirpath, n)),
                        None => (),
                    }
                    TreeWalkResult::Ok
                })?;
            }
        },
        Err(e) => return Err(Box::new(e)),
    }
    Ok(upgrades)
}

/// Get the list of ugprades needed to upgrade from "from" to "to" branches
///
/// Uses the private get_branch_upgrades function.
///
/// Returns a vector of Strings with the upgrades in the "to" branch
/// that do not exist in the "from" branch on success. Returns the
/// error on failure.
pub fn get_upgrades(repo: &Repository, from: &Branch, to: &Branch) -> Result<Vec<String>, Box<dyn Error>> {
    let from_upgrades: Vec<String> = get_branch_upgrades(repo, from)?;
    let to_upgrades: Vec<String> = get_branch_upgrades(repo, to)?;
    let upgrades: Vec<String> = to_upgrades.into_iter().filter(|item| !from_upgrades.contains(item)).collect();
    Ok(upgrades)
}

/// Read a file and write its contents to the output file
///
/// Read a file (inf) and write its entire contents to the output file
/// handle (outf).
///
/// Returns an error on failure or an empty result on success.
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
/// Returns an error on failure or an empty Result on success.
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
/// Returns an empty result on success.
pub fn review_file(file: &String) -> Result<(), Box<dyn Error>> {
    let editor = match var("EDITOR") {
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
