# Docker Installation

```sh
# start up the services
$ docker-compose up --detach

# Perform database migrations
$ docker-compose exec --workdir /usr/src/adaptarr backend diesel --database-url "postgres://postgres:docker@db/postgres" --config-file /usr/src/adaptarr/diesel.toml migration --migration-dir /usr/src/adaptarr/migrations run

# start for real
$ docker-compose restart

# add a user
$ docker-compose exec backend /usr/bin/adaptarr user add admin@localhost --administrator --name admin --password admin
```


# Log in!

Now, visit http://localhost:8080 (the initial load takes a while) and log in using the email `admin@localhost` with `admin` as the password. 

To create teams, you will need to temporarily elevate your permissions (like sudo). Visit http://localhost:8080/elevate to do that.

When you add yourself to a team be sure to check the terminal for an invite link. Rather than sending you an email, `config.toml` sends the email to your terminal.

Once you add a team, you can add a book. Download a complete zip file from cnx.org go to the books tab, click the lock icon and then the plus icon, select a team, and attach the zip file. Many files should be created in the `./files/` directory in adaptarr-server.






Now you can import a book, but in order to see the book you have to add yourself to a team.
