# Hawkeye - Batch Monitor

<img src="https://github.com/user-attachments/assets/05eb1aca-fd28-4a25-a64c-cc76329b70c1"></img>

### About
A batch job monitoring solution designed for use with the [PBS Professional by Altair](https://altair.com/pbs-professional) built and designed by the [Center for Research Computing and Data](https://www.niu.edu/crcd/index.shtml) (CRCD) for the [Metis Supercomputing Cluster](https://www.niu.edu/crcd/images/metislayoutandspecification.pdf) at NIU.

This tool monitors jobs by parsing and storing the output of the `jobstat -anL` and `jmanl <username> year raw` commands in a persistant database, and allows you to view this data via a convenient [web application](https://hawkeye.hiibolt.com).

Authentication and data access is done based on the user's actual login to the Metis cluster, which is securely and remotely verified over SSH. Similarly, commands are also run over SSH, which means this software can be deployed anywhere.

## Implementation Details
### Monolithic Architecture
This web application and its backend are written entirely in [Rust](https://www.rust-lang.org/). Rust was selected as the language, as parsing sensitive data on a scale this large requires a highly performant and secure solution - both of which are strong selling points of Rust. 

One of the newer innovations in frontend development is [Server Side Rendering](https://www.sanity.io/glossary/server-side-rendering) (SSR). This has many benefits, one of the largest being faster loading times. It is possible for hundreds or even thousands of jobs to be displayed on one page; and with Client Side Rendering (CSR), this would normally be handled with an API call. This API call can slow down the browser, and possibly create a janky experience for the user as they stare at a blank page while the page loads. 

By using the [Askama](https://github.com/rinja-rs/askama) Rust framework, this data can be directly injected and pre-rendered into the returned HTML.
### Web Server and Authentication
The web server for this application is built on the [Axum](https://github.com/tokio-rs/axum) Rust framework. Axum is well-suited for creating safe, highly parallel, and extremely performant web servers.

Authentication is handled by remotely executing an `expect` script for the `su` command over SSH, done with the [`openssh`](https://github.com/openssh-rust/openssh) crate. Sessions are stored with the [`tower-sessions`](https://github.com/maxcountryman/tower-sessions) crate. Because of the extremely sensitive nature of the credentials, both the credentials themselves and the sessions are only stored in memory - and sessions expire after 30 minutes.
### Command Execution and Persistent Storage
Command execution is done remotely over SSH, after which the command output is parsed with the [`regex`](https://github.com/rust-lang/regex) crate.

Data from `jobstat`, `jmanl`, and `groups` is stored persistantly in a SQLite database via the [`rusqlite`](https://github.com/rusqlite/rusqlite) crate. Commands are run in parallel using the asynchronus Rust framework [Tokio](https://tokio.rs/).
### CI/CD, Build Process, and Containerization
This application and its dependancies are declaratively defined using the [Nix Package Manager](https://nixos.org/) and hash-locked using [Nix Flakes](https://wiki.nixos.org/wiki/Flakes). You can enter the development environment for it with `nix develop .#hawkeye`, or build the application wtih `nix build .#hawkeye`.

This application is built into a reproducible [Docker container image](https://www.docker.com/) using GitHub Actions (tutorial [here](https://github.com/docker/build-push-action)), and is publically available at `ghcr.io/hiibolt/hawkeye:latest`.
## Usage
This application can be deployed either using the standalone container image at `ghcr.io/hiibolt/hawkeye:latest` or with [Docker Compose](https://docs.docker.com/compose/). 

By default, this application exposes itself on post 5777 and on the host network.

### Docker Compose
Firstly, clone this respository and move into it:
```
git clone https://github.com/hiibolt/hawkeye.git
cd hawkeye
```

Next, create a `.env` file, and add the following variables:
- **Required Variables**
- `REMOTE_USERNAME` - The SSH username you'd like to use
- `REMOTE_HOSTNAME` - The hostname of the remote machine 
- `DB_PATH` - The location of the DB you'd like to open from, relative to the `data` volume. You can leave this as `data.db`, if you don't know what to do. It will create a new database for you.
- **Optional Variables**
- `RUST_LOG` - The max level of logging to use. Some options are `info`, `warn`, and `error`.
- `GROUPS_DAEMON_PERIOD` - The time in seconds between each groups daemon run. The default is an hour.
- `JOBS_DAEMON_PERIOD` - The time in seconds between each data gathering (`jobstat`). Default is every 5 minutes.
- `OLD_JOBS_DAEMON_PERIOD` - The time in seconds between each data verification (`jmanl`). Default is every 30 minutes.

Deploying is as simple as running `docker compose up -d`. Please note that it will may take substantial time to pull the image.

Next, you'll need to enter the container and generate an SSH keyfile:
```
docker exec -it hawkeye-hawkeye-1 /bin/sh
ssh-keygen
ssh-copy-id <remote_username>@<remote_hostname>
exit
```

Finally, restart the stack to include the new SSH login:
```
docker compose restart
```