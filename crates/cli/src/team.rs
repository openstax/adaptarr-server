use adaptarr_models::{Model, User, Role, Team, db, permissions::TeamPermissions};
use failure::{Error, format_err};
use structopt::StructOpt;
use std::collections::HashMap;
use diesel::connection::Connection as _;

use crate::{
    Config,
    Result,
    user::RoleArg,
    util::{format, parse_permissions, print_table},
};

#[derive(StructOpt)]
pub struct Opts {
    /// Team to inspect
    team: Option<i32>,
    #[structopt(subcommand)]
    command: Option<Command>,
}

#[derive(StructOpt)]
pub enum Command {
    /// List teams
    #[structopt(name = "list")]
    List,
    /// Add a new team
    #[structopt(name = "add")]
    Add(AddOpts),
    /// Manage roles
    #[structopt(name = "role")]
    Role(RoleOpts),
    /// Manage members
    #[structopt(name = "member")]
    Member(MemberOpts),
}

pub fn main(cfg: &Config, opts: Opts) -> Result<()> {
    if opts.team.is_none() && opts.command.is_none() {
        Opts::clap().print_help()?;
        return Ok(())
    }

    if opts.command.is_none() {
        return inspect(cfg, &opts);
    }

    match opts.command.as_ref().unwrap() {
        Command::List => list(cfg),
        Command::Add(add_opts) => add_team(cfg, add_opts),
        Command::Role(role_opts) => role(cfg, &opts, role_opts),
        Command::Member(member_opts) => member(cfg, &opts, member_opts),
    }
}

fn inspect(cfg: &Config, opts: &Opts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let team = opts.team(&db)?;

    println!("ID:   {}", team.id());
    println!("Name: {}", team.name);

    println!("\nRoles:");

    let roles: HashMap<i32, Role> = team.get_roles(&db)?
        .into_iter()
        .inspect(|role| {
            println!("  - {} {}: {}", role.id, role.name, format(role.permissions()));
        })
        .map(|role| (role.id(), role))
        .collect();

    println!("\nMembers:");

    for member in team.get_members(&db)? {
        let user = member.get_user(&db)?;
        print!("  - {} {}", user.id, user.name);

        if let Some(role_id) = member.role {
            let role = roles.get(&role_id).unwrap();
            print!(", role: {}", role.name);
        }

        println!(", permissions: {}", format(member.permissions()));
    }

    Ok(())
}

fn list(cfg: &Config) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let teams = Team::all(&db)?;

    let rows = teams.iter()
        .map(|team| (team.id.to_string(), team.name.as_str()))
        .collect::<Vec<_>>();

    print_table(("ID", "Name"), &rows);

    Ok(())
}

#[derive(StructOpt)]
pub struct AddOpts {
    /// Team's name
    name: String,
}

fn add_team(cfg: &Config, opts: &AddOpts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let team = Team::create(&db, &opts.name)?;

    println!("Created team {}", team.id);

    Ok(())
}

#[derive(StructOpt)]
pub struct RoleOpts {
    #[structopt(subcommand)]
    command: RoleCommand,
}

#[derive(StructOpt)]
pub enum RoleCommand {
    /// List roles
    #[structopt(name = "list")]
    List,
    /// Add a role
    #[structopt(name = "add")]
    Add(AddRoleOpts),
    /// Remove a role
    #[structopt(name = "remove")]
    Remove(RemoveRoleOpts),
    /// Modify a role
    #[structopt(name = "modify")]
    Modify(ModifyRoleOpts),
}

fn role(cfg: &Config, opts: &Opts, role: &RoleOpts) -> Result<(), Error> {
    match role.command {
        RoleCommand::List => list_roles(cfg, opts),
        RoleCommand::Add(ref add) => add_role(cfg, opts, add),
        RoleCommand::Remove(ref remove) => remove_role(cfg, opts, remove),
        RoleCommand::Modify(ref modify) => modify_role(cfg, opts, modify),
    }
}

fn list_roles(cfg: &Config, opts: &Opts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let team = opts.team(&db)?;
    let roles = team.get_roles(&db)?;

    let rows = roles.iter()
        .map(|role| (
            role.id.to_string(),
            role.name.as_str(),
            format(role.permissions()),
        ))
        .collect::<Vec<_>>();

    print_table(("ID", "Name", "Permissions"), &rows);

    Ok(())
}

#[derive(StructOpt)]
pub struct AddRoleOpts {
    /// Role's name
    name: String,
    /// Role's permissions
    #[structopt(long = "permissions", parse(try_from_str = parse_permissions))]
    permissions: Option<TeamPermissions>,
}

fn add_role(cfg: &Config, opts: &Opts, add: &AddRoleOpts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let team = opts.team(&db)?;
    let permissions = add.permissions.unwrap_or_else(TeamPermissions::empty);
    let role = Role::create(&db, &team, &add.name, permissions)?;

    println!("Created role {}", role.id);

    Ok(())
}

#[derive(StructOpt)]
pub struct RemoveRoleOpts {
    /// ID of the role to remove
    role: i32,
}

fn remove_role(cfg: &Config, opts: &Opts, remove: &RemoveRoleOpts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let team = opts.team(&db)?;

    team.get_role(&db, remove.role)?.delete(&db)?;

    Ok(())
}

#[derive(StructOpt)]
pub struct ModifyRoleOpts {
    /// ID of the role to modify
    role: i32,
    /// Set role's permissions
    #[structopt(long = "permissions", parse(try_from_str = parse_permissions))]
    permissions: Option<TeamPermissions>,
}

fn modify_role(cfg: &Config, opts: &Opts, modify: &ModifyRoleOpts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let team = opts.team(&db)?;
    let mut role = team.get_role(&db, modify.role)?;

    db.transaction(|| {
        if let Some(permissions) = modify.permissions {
            role.set_permissions(&db, permissions)?;
        }

        Ok(())
    })
}

#[derive(StructOpt)]
pub struct MemberOpts {
    #[structopt(subcommand)]
    command: MemberCommand,
}

#[derive(StructOpt)]
pub enum MemberCommand {
    /// List members
    #[structopt(name = "list")]
    List,
    /// Add a member
    #[structopt(name = "add")]
    Add(AddMemberOpts),
    /// Remove a member
    #[structopt(name = "remove")]
    Remove(RemoveMemberOpts),
    /// Modify a member
    #[structopt(name = "modify")]
    Modify(ModifyMemberOpts),
}

fn member(cfg: &Config, opts: &Opts, member: &MemberOpts) -> Result<(), Error> {
    match member.command {
        MemberCommand::List => list_members(cfg, opts),
        MemberCommand::Add(ref add) => add_member(cfg, opts, add),
        MemberCommand::Remove(ref remove) => remove_member(cfg, opts, remove),
        MemberCommand::Modify(ref modify) => modify_member(cfg, opts, modify),
    }
}

fn list_members(cfg: &Config, opts: &Opts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let team = opts.team(&db)?;
    let members = team.get_members(&db)?;

    let rows = members.into_iter()
        .map(|member| {
            let user = member.get_user(&db)?;
            let permissions = member.permissions();
            let (_, role) = member.into_db();

            Ok((
                user.id().to_string(),
                user.into_db().name,
                role.map(|role| role.name).unwrap_or_else(String::new),
                format(permissions),
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    print_table(("ID", "Name", "Role", "Permissions"), &rows);

    Ok(())
}

#[derive(StructOpt)]
pub struct AddMemberOpts {
    /// ID of the user to add as a member
    user: i32,
    /// Member's permissions
    #[structopt(long = "permissions", parse(try_from_str = parse_permissions))]
    permissions: Option<TeamPermissions>,
    /// Member's role
    #[structopt(long = "role")]
    role: Option<i32>,
}

fn add_member(cfg: &Config, opts: &Opts, add: &AddMemberOpts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let mut team = opts.team(&db)?;
    let user = User::by_id(&db, add.user)?;
    let role = add.role.map(|id| team.get_role(&db, id)).transpose()?;
    let permissions = add.permissions.unwrap_or_else(TeamPermissions::empty);

    team.add_member(&db, &user, permissions, role.as_ref())?;

    println!("User {} {} added as a member", user.id(), user.name);

    Ok(())
}

#[derive(StructOpt)]
pub struct RemoveMemberOpts {
    /// ID of the user to remove
    user: i32,
}

fn remove_member(cfg: &Config, opts: &Opts, remove: &RemoveMemberOpts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let team = opts.team(&db)?;
    let user = User::by_id(&db, remove.user)?;

    team.get_member(&db, &user)?.delete(&db)?;

    Ok(())
}

#[derive(StructOpt)]
pub struct ModifyMemberOpts {
    /// ID of the user whose membership to modify
    user: i32,
    /// Set member's permissions
    #[structopt(long = "permissions", parse(try_from_str = parse_permissions))]
    permissions: Option<TeamPermissions>,
    /// Set member's role
    #[structopt(long = "role")]
    role: Option<RoleArg>,
}

fn modify_member(cfg: &Config, opts: &Opts, modify: &ModifyMemberOpts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let team = opts.team(&db)?;
    let user = User::by_id(&db, modify.user)?;
    let mut member = team.get_member(&db, &user)?;

    db.transaction(|| {
        if let Some(permissions) = modify.permissions {
            member.set_permissions(&db, permissions)?;
        }

        if let Some(ref role) = modify.role {
            let role = role.get(&db, &team)?;
            member.set_role(&db, role)?;
        }

        Ok(())
    })
}

impl Opts {
    fn team_id(&self) -> Result<i32> {
        match self.team {
            Some(uuid) => Ok(uuid),
            None => Err(format_err!("This command requires a team")),
        }
    }

    fn team(&self, db: &db::Connection) -> Result<Team> {
        Team::by_id(db, self.team_id()?).map_err(Into::into)
    }
}
