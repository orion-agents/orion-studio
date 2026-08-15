# Orion Studio collaboration server

This crate contains the back-end logic used by Orion Studio's real-time
collaboration features. It can be run locally or deployed by an operator.

No public Orion collaboration or authentication endpoint is assumed to be
available. A deployment must provide and configure its own authentication,
WebSocket endpoint, database, LiveKit service, secrets, and operational
controls. The desktop client must not silently fall back to a Zed-hosted
service.

# Local Development

## Database setup

Before you can run the collaboration server locally, set up its PostgreSQL
database. The bootstrap scripts still use the database name `zed`; this is a
legacy implementation identifier, not the Orion Studio product name.

1. Ensure you have postgres installed. If not, install with `brew install postgresql@15`.
2. Follow the steps on Brew's formula and verify your `$PATH` contains `/opt/homebrew/opt/postgresql@15/bin`.
3. If you hadn't done it before, create the `postgres` user with `createuser -s postgres`.
4. You are now ready to run the `bootstrap` script:

```sh
script/bootstrap
```

This script sets up the legacy-named `zed` PostgreSQL database and populates it
with development users. It requires internet access because it fetches selected
users from the GitHub API.

The script will create several _admin_ users, who you'll sign in as by default when developing locally. The GitHub logins for the default users are specified in the `seed.default.json` file.

To use a different set of admin users, create `crates/collab/seed.json`.

```json
{
  "admins": ["yourgithubhere"],
  "channels": ["orion-studio"]
}
```

## Testing collaborative features locally

In one terminal, run the Orion Studio collaboration server and the LiveKit
development server:

```sh
foreman start
```

In a second terminal, run two or more local Orion Studio instances:

```sh
script/orion-studio-local -2
```

`script/orion-studio-local` starts one to six Orion Studio instances. The
`-2`, `-3`, and `-4` flags are common layouts; each instance connects to the
local `collab` server and signs in as a different development user from
`seed.json` or `seed.default.json`.

`script/zed-local` remains as a minimal legacy developer compatibility shim and
forwards all arguments to the canonical script.

# Deployment

The repository includes deployment helpers for operator-managed staging and
production environments. They do not prove that an Orion service is currently
deployed. Before using them, configure the target cluster, DNS, TLS,
authentication, secrets, database, backups, and monitoring.

After those prerequisites are in place, deployment can be triggered with:

- `./script/deploy-collab staging`
- `./script/deploy-collab production`

Use `./script/what-is-deployed` to inspect the configured target. Treat an
unconfigured or unreachable target as not deployed.
