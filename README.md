# Domain watcher
A simple CLI program that lurks on your machine, consumes virtually no resources, checks a whois provider now and then and notifies you if your domain of dreams becomes available.
## You will need
- a machine running Linux connected to the internet with `screen` package installed
- an  email account that can be used with SMTP
- an account at [WhoisJson](https://whoisjson.com) and their API key. Other providers will possibly be supported in the future, I like this one for now.
## Installation and run
Copy the executable that you find here to your machine. That was the installation.

The program will block your terminal. So it is intended to run on the separated screen instance.

Create a new screen:
~~~
screen -S "domain_watcher"
~~~
Run it with
~~~
./domain_watcher
~~~
and feed it with your SMTP credentials, the email the notifications will be sent to, domain to watch and the API key. It will create a config file `config.toml` that you can edit later to your likings. If you delete it, the program will ask you to fill out the data again. If you choose to log data, the program will create a log file named `domain_watcher.log`.

After you are done initializing it and you check that you received the test mail, you can <kbd>Ctrl</kbd> + <kbd>A</kbd> + <kbd>D</kbd> to detach from the screen and go on with your day. If you want to check on it, just type `screen -r domain_watcher` to see what it writes or terminate it by <kbd>Ctrl</kbd> + <kbd>C</kbd>.
## Some thoughts
The program was intended to run on a VPS, a server or a machine that is already running. Other situations are probably not worth it.

You will be okay if the program asks the API provider once a day (86400 seconds). It is extremely unlikely that the domain will become available until it expires, so there is no need to spam the API provider with unnecessary requests.
