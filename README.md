# mkdbupgrade

## Description

mkdbupgrade generates custom database upgrade scripts between two
arbitrary releases of the [Evergreen ILS](https://evergreen-ils.org/).
Most sites do not neatly upgrade from versions with community
supported database upgrades, so this program exists to allow you to
create your own.

It also permits you to add arbitrary SQL to run both before and after
the main upgrade script.  It has options to move pesky upgrades out of
the main transaction into individual transactions so that they do not
fail and blow up your entire upgrade.

## Getting Started

### Dependencies

mkdbupgrade is a Rust program, so it requires
[Rust](https://www.rust-lang.org/) and
[cargo](https://lib.rs/crates/cargo) to build it.

mkdbupgrade uses the [clap](https://docs.rs/clap/latest/clap/) and
[regex](https://docs.rs/regex/latest/regex/) crates.  Cargo will take
care of adding these for you.

### Installing

The [cargo
install](https://doc.rust-lang.org/cargo/commands/cargo-install.html)
command will take care of it.

## Running the Program

mkdbupgrade expects to be run from a local clone of the Evergreen git
repository.  The branch for the Evergreen version to which you are
upgrading should be checked out, example:

```
git checkout -b rel_3_15_4 origin/tags/rel_3_15_4
```

If you have a custom branch, that's even better, just make sure your
current branch has the code that you expect to upgrade to.

mkdbupgrade attempts to determine the new version of Evergreen from
the branch name.  If your branch has a series of 3 numbers (1 or 2
digits each) separated by underscores with a leading underscore as in
the example above, then mkdbupgrade will be able to auto detect the
version.  If this is not the case, you can specify the version number
with the `-v` option.  Note that specifying the version this way allow
you to use anything for the version, including nonnumerical strings.
In the case where mkdbupgrade detects the version, it will be
converted to a string with periods replacing the underscores:
"3.15.14" with the above example.

The only option that is absolutely required is `-f` to specify the
branch from which you are making the upgrade script.  For example,
with the above branch checked out you could make a custom database
upgrade from Evergreen 3.14.5 with the following invocation:

```
mkdbupgrade -f origin/tags/rel_3_14_5
```

As you can see from the above, the from branch does not have to be
local, but it does have to be accessible from your current clone.  If
not, mkdbupgrade will complain and shut down.

mkdbupgrade attempts to determine the Evergreen version from which you
are upgrading using the branch name in the same manner as it does with
the target branch.  If your branch lacks the version, or if you wish
to call it something else, you can specify the version of the from
branch with the `-F` option.

Database upgrades sometimes need to be run out of order because they
conflict with others when merged into one big transaction.
mkdbupgrade has the `-m` option that allows you to specify strings
that match patterns in the filenames of these database upgrades.  The
`-m` option can be used more than once when there are more than one
such upgrade.  For instance, when upgrading from Evergreen 3.7.4 to
3.15.4 the following database upgrades have this kind of conflict:

  * 1312.schema.add_editor_index_to_usr_message.sql
  * 1433.schema.multifactor-auth.sql
  * 1461.schema.phone-settings-index.sql
  * 1465.schema.staff-portal-urls-newtab.sql

You can handle this by adding the `-m` option for each of these
upgrades.  Specifying the upgrade version number is usually enough.

```
mkdbupgrade -f origin/tags/rel_3_7_4 -m 1312 -m 1433 -m 1461 -m 1465
```

Each of these upgrades will be set aside when the main transaction
block is being built and will be added after the main transaction
exactly as it appears in its file without having any `BEGIN` or
`COMMIT` lines removed.  (Upgrades are normally merged into a single
transaction when added to the database upgrade script and have their
`BEGIN` and `COMMIT` lines removed.)

> How do you know when you need to move an upgrade outside of the main transaction?

> Experience.  You usually find out by making a database upgrade,
> running it on a test database, and it fails.

You can add arbitrary code to run before or after the upgrade script
proper.  This is useful if you have some cleanup to do before or after
running the upgrade.  You might want to add new permissions to
existing user groups or add values for new organizational unit
settings, etc.

You can add any number of SQL files to add before running the upgrade
with `-p` option followed by the filename.

Code to run after can be added with the `-a` option.

We might extend our previous upgrade by adding some code to run before
and after:

```
mkdbupgrade -f origin/tags/rel_3_7_4 -m 1312 -m 1433 -m 1461 -m 1465 \
-p ~/src/sql/prepend-to-upgrade.sql -a ~/src/sql/append-to-upgrade.sql
```

mkdbupgrade names the upgrade scripts just like the existing Evergreen
database upgrades: `A.B.C-X.Y.Z-upgrade-db.sql`.  You can add a custom
prefix to the filename with the `-P` option. (NOTE: it is a capital P
for this option and the prepend option is a lowercase p.)  The
following command would produce a file named
`cwmars_custom_3.7.4-3.15.4-upgrade-db.sql`:

```
mkdbupgrade -f origin/tags/rel_3_7_4 -m 1312 -m 1433 -m 1461 -m 1465 \
-p ~/src/sql/prepend-to-upgrade.sql -a ~/src/sql/append-to-upgrade.sql \
-P cwmars_custom_
```

mkdbupgrade writes its output file in the
`Open-ILS/src/sql/Pg/upgrade` directory of your Evergreen repository
by default.  You may change the destination directory with the `-O`
option, for example `-O ~/src/sql` would put the above output file in
`~/src/sql/cwmars_custom_3.7.4-3.15.4-upgrade-db.sql`.

mkdbupgrade will not overwrite an existing upgrade script unless you
specify the `-C` option.  This is a flag that takes no argument and
tells mkdbupgrade to clobber any existing file with the same name.
Use this option with caution, though it can be useful if you're
testing and the previous upgrade did not work.

Finally, you can review the resulting file in your editor with the
`-r` option.  This will tell mkdbupgrade to open the file with the
program specified in the `EDITOR` environment variable.

In the event of any errors or instances where an option is required
but missing, mkdbupgrade will generally fail and print a hopefully
useful message suggesting that you need to add an option or that it
could not find a file, etc.

A synopsis of the options and basic help is available with the `-h` or
`--help` flags.

## TODO

Add tests and github workflow to run them.

## Authors

[Jason Stephenson](https://github.com/Dyrcona)

## License

mkdbupgrade is licensed  under the terms of the GPL2 - see [LICENSE.txt](LICENSE.txt) file for details.

## Acknowledgments

mkdbupgrade was implemented by the author while working for [C/W MARS,
Inc.](https://www.cwmars.org/) to ease the burden of preparing
Evergreen ILS upgrades.
