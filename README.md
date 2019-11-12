# macOS Installation

This uses a Docker version of Postgres but runs a local server, frontend, and nginx.

We will do the following in order:

1. install system dependencies
1. install, configure, and run the server
1. add a user
1. install and run the frontend
1. configure nginx and run it
1. visit `http://adaptarr.test/`, log in, add a team, import a book, edit the content

## System dependencies

```
brew install libmagic
brew install nginx
brew install rust # or follow the instructions at https://www.rust-lang.org/tools/install
```

Change the [line in crates/models/src/models/file.rs](https://github.com/openstax-poland/adaptarr-server/blob/master/crates/models/src/models/file.rs) to `cookie.load(&["/usr/local/share/misc/magic"])`


# Start up a Postgres instance in Docker

If you use this option then make sure the config.toml file points to this instance (see below).


# Example config.toml file

Create a `config.toml` file like the following.

Notes:

- `domain =` needs to match the entry in `/etc/hosts` later on.
- The combination of `transport = "log"` and `level = "debug"` allows you to see the invite link that is sent to users (so you can click it)
- `url = "..."` connects to Postgres running on docker. if you have a different setup, change it here
- `secret = "base64:...` arbitrary, but necessary

```
# :required: Server configuration.
[server]
# :optional: Internet address and port on which to listen.
address = "127.0.0.1:8080"
# :required: Domain (host name) of this server.
domain = "adaptarr.test"
# :optional: Secret key, used e.g. to sign session cookies. Must be at least 32
# bytes long.
#
# Value can be specified either as a base64-encoded string (in which case it
# must be prefixed with "base64:"), or as path to a file containing the value
# (in which it must be prefixed with "file:").
#
# If omitted a random secret will be generated each time the server is started.
#
# You can generate a new secure secret with
# $ echo base64:`openssl rand -base64 32`
secret = "base64:7iTb+q31r3ZBhydC7MCVWQcuimzzF1gE6S5/I14D4bo="

# :optional: Database configuration.
[database]
# :required: Database url. This field is overridden by the DATABASE_URL
# env variable.
url = "postgres://postgres:docker@localhost/postgres"

# :required: Mailing configuration.
[mail]
# :required: Email address to send messages as.
#
# Address can be anything understood as an address according to RFC 5322 (per
# the mailbox production in §3.4), in particular:
# - bare email address, e.g. noreply@example.com
# - email address with sender's name, e.g. Name <noreply@example.com>
sender = "Name <noreply@example.com>"
# :required: What method to use to send mails. Possible values are
# - "log" to log emails to standard error for debugging,
# - "sendmail" to use the sendmail(1) command,
# - "smtp" to use SMTP
transport = "log"

# Following options are only available for SMTP transport.

# :required: Name of the SMTP host server to connect to.
host = "mail.example.com"
# :optional: Port to connect to. Defaults to 25.
port = 25
# :optional: TLS configuration. Possible values are:
# - false: never use TLS
# - true: use TLS when available, otherwise fall back to unencrypted
# - "strict" or "always": always use TLS, don't allow unencrypted connections
use-tls = true

# :required: Backing storage for files.
[storage]
# :required: Path to a directory in which user-uploaded files will be kept.
path = "./files/"

# :optional: Logging configuration
#
# There are six possible logging levels: "off" disables logging altogether,
# "error" logs only critical errors, "warn"logs non-critical errors
# and warnings, "info" logs informational messages, "debug" logs verbose
# debugging messages, and "trace" log extremely verbose debugging messages. Each
# log level also includes messages from all previous (more specific) log levels.
[logging]
# :optional: Default logging level. This applies to all logs for which there
# isn't a more specific setting, or for which a more specific setting exists
# but is not set.
#
# Default value if "info"
level = "debug"
# :optional: Logging level for the networking layer. Set to "info" to display
# incoming requests. Unset by default.
network = "trace"

# :optional: Custom filters.
#
# This can also be set through the RUST_LOG environmental variable.
[logging.filters]
# Example: only show incoming requests, silence all other networking logs.
"actix_net" = "off"
"actix_web::middleware::logger" = "info"

# :optional: Sentry.io configuration
#[sentry]
# :required: Client key.
#dns = "https://key@instance/project"
```

1. start the server (`$ RUST_LOG=trace cargo run --release server start`) in release mode (so DB migrations run) and with verbose logging (to see the email invite link). It should start by running database migrations
1. now you can create the first user: `$ cargo run user --administrator add`. You don't have to stop the server to do this
5. at this point you should have a working server

If you see the following...

```
$ cargo run server start
    Finished dev [unoptimized + debuginfo] target(s) in 1.35s
     Running `target/debug/adaptarr server start`
[2019-10-09T14:27:00Z INFO  actix_server::builder] Starting 4 workers
[2019-10-09T14:27:00Z INFO  actix_server::builder] Starting server on 127.0.0.1:8080
[2019-10-09T14:27:00Z ERROR adaptarr_models::processing::xref_targets] Could not process stale documents: relation "modules" does not exist
```

then be sure to run `cargo run --release server start` to run the DB migrations.

# Run the server in non-release mode

so that HTTPS is not forced: `RUST_LOG=trace cargo run server start`


## Verify Server is running


run something like this to verify that the server is running (maybe check `/login` or `/reset`):

```
curl --head http://localhost:8182
HTTP/1.1 404 Not Found
content-length: 0
date: Wed, 09 Oct 2019 15:25:45 GMT
```


# Set up the frontend

To set up the frontend I used an older version of the code that works well-enough that it runs (it does not use the custom packages in the custom npm registry)

Use the `phil-works` branch of [the fork of adaptarr-front](https://github.com/philschatz/adaptarr-front/tree/phil-works).

Run `npm install && npm start`


# Set up nginx

Edit `/etc/hosts` to contain:

```
0.0.0.0 adaptarr.test

# Maybe the following are also useful, not sure:
# 0.0.0.0 adaptarr
# 0.0.0.0 frontend
```

## On macOS

Edit `/usr/local/etc/nginx/servers/adaptarr.test` to say:

```
upstream adaptarr {
    server 0.0.0.0:8080;
}
upstream front {
    server 0.0.0.0:3000;
}
server {
    listen 80;
    listen [::]:80;
    server_name adaptarr.test;
    root /dev/null;
    try_files $uri @front;
    client_max_body_size 400M;
    location @front {
        proxy_set_header X-Forwarded_Proto $scheme;
        proxy_set_header Host $http_host;
        proxy_pass http://front;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_redirect http:// $scheme://;
    }
    location ~ ^/api/v1/(events|conversations/.+/socket) {
   	proxy_set_header X-Forwarded_Proto $scheme;
   	proxy_set_header Host $http_host;
   	proxy_pass http://adaptarr;
   	proxy_http_version 1.1;
   	proxy_set_header Upgrade $http_upgrade;
    	proxy_set_header Connection "Upgrade";
    }
    location ~ ^/(login|logout|api|register|reset|join|elevate) {
        proxy_set_header X-Forwarded_Proto $scheme;
        proxy_set_header Host $http_host;
        proxy_pass http://adaptarr;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_redirect http:// $scheme://;
    }
}
```


# Log in!

Now, visit http://adaptarr.test and log in. Then you will need to create a team and invite yourself to it.

To create teams, you will need to temporarily elevate your permissions (like sudo). Visit http://adaptarr.test/elevate to do that.

When you add yourself to a team be sure to check the terminal for an invite link. Rather than sending you an email, `config.toml` sends the email to your terminal.

Once you add a team, you can add a book. Download a complete zip file from cnx.org go to the books tab, click the lock icon and then the plus icon, select a team, and attach the zip file. Many files should be created in the `./files/` directory in adaptarr-server.






Now you can import a book, but in order to see the book you have to add yourself to a team.
