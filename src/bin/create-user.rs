use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use expenses::{
    model::Role,
    server::{audit, auth::hash_password, config::Config, db},
};
use serde_json::json;

#[derive(Clone, Copy, ValueEnum)]
enum RoleArg {
    User,
    Admin,
}

/// Create a local (non-Microsoft) user for the expense system.
#[derive(Parser)]
#[command(name = "create-user")]
struct Args {
    /// Login name (lowercase letters, digits, '.', '_' or '-').
    #[arg(long)]
    username: String,
    #[arg(long, value_enum)]
    role: RoleArg,
    /// Name shown in the UI; defaults to the username.
    #[arg(long)]
    display_name: Option<String>,
}

const MIN_PASSWORD_LEN: usize = 12;

fn valid_username(u: &str) -> bool {
    (3..=64).contains(&u.len())
        && u.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let username = args.username.trim().to_lowercase();
    if !valid_username(&username) {
        bail!("invalid username: use 3-64 chars of a-z, 0-9, '.', '_' or '-'");
    }
    let role = match args.role {
        RoleArg::User => Role::User,
        RoleArg::Admin => Role::Admin,
    };

    let config = Config::from_env()?;
    let pool = db::connect(&config).await?;

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE auth_provider = 'local' AND username = ?)")
            .bind(&username)
            .fetch_one(&pool)
            .await?;
    if exists {
        bail!("local user {username:?} already exists");
    }

    let password = rpassword::prompt_password("Password: ")?;
    if password.chars().count() < MIN_PASSWORD_LEN {
        bail!("password must be at least {MIN_PASSWORD_LEN} characters");
    }
    if rpassword::prompt_password("Repeat password: ")? != password {
        bail!("passwords do not match");
    }
    let hash = hash_password(&password).map_err(|e| anyhow::anyhow!("hashing password: {e}"))?;

    let mut tx = pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (auth_provider, username, display_name, password_hash, role)
         VALUES ('local', ?, ?, ?, ?) RETURNING id",
    )
    .bind(&username)
    .bind(args.display_name.as_deref().unwrap_or(&username))
    .bind(&hash)
    .bind(role.as_str())
    .fetch_one(&mut *tx)
    .await
    .context("inserting user")?;
    audit::record(
        &mut *tx,
        None,
        "user_created",
        Some(("user", &id.to_string())),
        Some(json!({"via": "create-user", "username": username, "role": role.as_str()})),
        None,
    )
    .await?;
    tx.commit().await?;

    println!("Created {} user {username:?} (id {id})", role.as_str());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::valid_username;

    #[test]
    fn usernames() {
        assert!(valid_username("bjarke.fs"));
        assert!(!valid_username("ab"));
        assert!(!valid_username("Bjarke"));
        assert!(!valid_username("a b c"));
    }
}
