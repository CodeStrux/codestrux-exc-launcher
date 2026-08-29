# Command examples

A categorized cookbook of `[[profiles.commands]]` entries you can copy into
your own `config.toml`. See the [Config](README.md#config) section of the
README for the full schema — in short, each entry needs a globally-unique
`name`, a `description` (shown by `exc list`), a `command` (run via
`sh -c`, with `{{placeholder}}` params substituted first), and an optional
list of `params` for anything the command needs to ask you.

Every snippet below is a drop-in entry for a `[[profiles]]` block, e.g.:

```toml
[[profiles]]
name = "system"
label = "System"
description = "Everyday local system commands"

  # paste any [[profiles.commands]] snippet from below here
```

Adjust names, flags, and paths for your own machine — these are starting
points, not gospel.

## System tools

```toml
  [[profiles.commands]]
  name = "mem-usage"
  description = "Show memory usage in human-readable form"
  command = "free -h || vm_stat"   # Linux, falls back to macOS's vm_stat

  [[profiles.commands]]
  name = "biggest-dirs"
  description = "List the 10 largest directories under the current path"
  command = "du -sh */ 2>/dev/null | sort -rh | head -n 10"

  [[profiles.commands]]
  name = "listening-ports"
  description = "Show all processes listening on a TCP/UDP port"
  command = "lsof -iTCP -sTCP:LISTEN -n -P || ss -tulpn"

  [[profiles.commands]]
  name = "kill-by-name"
  description = "Kill all processes matching a name"
  command = "pkill -f {{process_name}}"

    [[profiles.commands.params]]
    name = "process_name"
    prompt = "Process name (or pattern) to kill"
    default = ""

  [[profiles.commands]]
  name = "tail-syslog"
  description = "Tail the system log, filtered by a search term"
  command = "log stream --predicate 'eventMessage contains \"{{term}}\"' || journalctl -f | grep {{term}}"

    [[profiles.commands.params]]
    name = "term"
    prompt = "Filter term"
    default = "error"

  [[profiles.commands]]
  name = "clear-package-cache"
  description = "Clear the local package manager's download cache"
  command = "brew cleanup -s || sudo apt-get clean || sudo dnf clean all"

  [[profiles.commands]]
  name = "service-status"
  description = "Show the status of a system service/daemon"
  command = "systemctl status {{service}} || brew services info {{service}}"

    [[profiles.commands.params]]
    name = "service"
    prompt = "Service name"
    default = ""

  [[profiles.commands]]
  name = "restart-service"
  description = "Restart a system service/daemon"
  command = "sudo systemctl restart {{service}} || brew services restart {{service}}"

    [[profiles.commands.params]]
    name = "service"
    prompt = "Service name"
    default = ""
```

## Docker & containers

```toml
  [[profiles.commands]]
  name = "docker-exec-shell"
  description = "Open an interactive shell inside a running container"
  command = "docker exec -it {{container}} sh"

    [[profiles.commands.params]]
    name = "container"
    prompt = "Container name or id"
    default = ""

  [[profiles.commands]]
  name = "docker-compose-up"
  description = "Bring up the compose stack in the current directory, rebuilding images"
  command = "docker compose up -d --build"

  [[profiles.commands]]
  name = "docker-compose-down"
  description = "Tear down the compose stack, including volumes"
  command = "docker compose down -v"

  [[profiles.commands]]
  name = "docker-build-tagged"
  description = "Build the current directory into an image with a given tag"
  command = "docker build -t {{tag}} ."

    [[profiles.commands.params]]
    name = "tag"
    prompt = "Image tag (e.g. myapp:latest)"
    default = ""

  [[profiles.commands]]
  name = "docker-stats"
  description = "Live resource usage for running containers"
  command = "docker stats"

  # Note: Docker's --format flag uses Go template syntax ({{.Field}}), which
  # collides with exc's own {{param}} substitution — exc leaves any
  # {{...}} without a matching declared param untouched at run time, so the
  # command itself would still work, but `exc validate` flags it as an
  # unresolved placeholder. Piping/sorting plain table output sidesteps that.
  [[profiles.commands]]
  name = "docker-image-size"
  description = "List local images sorted by size"
  command = "docker images | tail -n +2 | sort -k7 -h"

  [[profiles.commands]]
  name = "docker-nuke-all"
  description = "Stop and remove every container, image, volume, and network (destructive)"
  command = "docker stop $(docker ps -aq) 2>/dev/null; docker system prune -af --volumes"
```

## Software development

```toml
  [[profiles.commands]]
  name = "run-tests-watch"
  description = "Run the test suite for the current project in watch mode"
  command = "cargo watch -x test || npm test -- --watch || pytest-watch"

  [[profiles.commands]]
  name = "format-project"
  description = "Auto-format the current project"
  command = "cargo fmt || npx prettier --write . || black ."

  [[profiles.commands]]
  name = "lint-project"
  description = "Run the linter for the current project"
  command = "cargo clippy --all-targets || npx eslint . || ruff check ."

  [[profiles.commands]]
  name = "new-python-venv"
  description = "Create and activate a fresh virtualenv in .venv"
  command = "python3 -m venv .venv; . .venv/bin/activate; pip install --upgrade pip"

  [[profiles.commands]]
  name = "npm-outdated"
  description = "List outdated npm dependencies in the current project"
  command = "npm outdated"

  [[profiles.commands]]
  name = "cargo-audit"
  description = "Scan Cargo dependencies for known security advisories"
  command = "cargo audit"

  [[profiles.commands]]
  name = "find-todo"
  description = "Grep the current project for TODO/FIXME comments"
  command = "grep -rn --exclude-dir={.git,node_modules,target} 'TODO\\|FIXME' ."

  [[profiles.commands]]
  name = "http-serve-dir"
  description = "Serve the current directory over HTTP on a given port"
  command = "python3 -m http.server {{port}}"

    [[profiles.commands.params]]
    name = "port"
    prompt = "Port"
    default = "8000"

  [[profiles.commands]]
  name = "diff-since-branch"
  description = "Show the diff against the point this branch diverged from another"
  command = "git diff $(git merge-base {{base}} HEAD)"

    [[profiles.commands.params]]
    name = "base"
    prompt = "Base branch"
    default = "main"
```

## Networking

```toml
  [[profiles.commands]]
  name = "ping-host"
  description = "Ping a host a fixed number of times"
  command = "ping -c 5 {{host}}"

    [[profiles.commands.params]]
    name = "host"
    prompt = "Host to ping"
    default = ""

  [[profiles.commands]]
  name = "traceroute-host"
  description = "Trace the network path to a host"
  command = "traceroute {{host}}"

    [[profiles.commands.params]]
    name = "host"
    prompt = "Host to trace"
    default = ""

  [[profiles.commands]]
  name = "dns-lookup"
  description = "Look up every common DNS record type for a domain"
  command = "dig {{domain}} ANY +noall +answer"

    [[profiles.commands.params]]
    name = "domain"
    prompt = "Domain"
    default = ""

  [[profiles.commands]]
  name = "http-headers"
  description = "Show the response headers for a URL"
  command = "curl -sSI {{url}}"

    [[profiles.commands.params]]
    name = "url"
    prompt = "URL"
    default = "https://"

  [[profiles.commands]]
  name = "my-public-ip"
  description = "Print this machine's public IP address"
  command = "curl -s https://api.ipify.org; echo"

  [[profiles.commands]]
  name = "ssh-tunnel"
  description = "Open a local port-forward tunnel through a jump host"
  command = "ssh -N -L {{local_port}}:{{remote_host}}:{{remote_port}} {{jump_host}}"

    [[profiles.commands.params]]
    name = "local_port"
    prompt = "Local port"
    default = "8080"

    [[profiles.commands.params]]
    name = "remote_host"
    prompt = "Remote host (as seen from the jump host)"
    default = "localhost"

    [[profiles.commands.params]]
    name = "remote_port"
    prompt = "Remote port"
    default = "80"

    [[profiles.commands.params]]
    name = "jump_host"
    prompt = "Jump host (user@host)"
    default = ""

  [[profiles.commands]]
  name = "speedtest"
  description = "Run a quick internet speed test"
  command = "curl -s https://raw.githubusercontent.com/sivel/speedtest-cli/master/speedtest.py | python3 -"
```

## Kubernetes & cloud

```toml
  [[profiles.commands]]
  name = "kubectl-pods"
  description = "List pods in a namespace"
  command = "kubectl get pods -n {{namespace}}"

    [[profiles.commands.params]]
    name = "namespace"
    prompt = "Namespace"
    default = "default"

  [[profiles.commands]]
  name = "kubectl-logs"
  description = "Tail logs for a pod"
  command = "kubectl logs -f {{pod}} -n {{namespace}}"

    [[profiles.commands.params]]
    name = "pod"
    prompt = "Pod name"
    default = ""

    [[profiles.commands.params]]
    name = "namespace"
    prompt = "Namespace"
    default = "default"

  [[profiles.commands]]
  name = "kubectl-shell"
  description = "Open a shell inside a pod's container"
  command = "kubectl exec -it {{pod}} -n {{namespace}} -- sh"

    [[profiles.commands.params]]
    name = "pod"
    prompt = "Pod name"
    default = ""

    [[profiles.commands.params]]
    name = "namespace"
    prompt = "Namespace"
    default = "default"

  [[profiles.commands]]
  name = "kubectl-port-forward"
  description = "Forward a local port to a service"
  command = "kubectl port-forward svc/{{service}} {{local_port}}:{{remote_port}} -n {{namespace}}"

    [[profiles.commands.params]]
    name = "service"
    prompt = "Service name"
    default = ""

    [[profiles.commands.params]]
    name = "local_port"
    prompt = "Local port"
    default = "8080"

    [[profiles.commands.params]]
    name = "remote_port"
    prompt = "Remote (service) port"
    default = "80"

    [[profiles.commands.params]]
    name = "namespace"
    prompt = "Namespace"
    default = "default"

  [[profiles.commands]]
  name = "aws-s3-ls"
  description = "List objects in an S3 bucket/prefix"
  command = "aws s3 ls s3://{{bucket_and_prefix}}"

    [[profiles.commands.params]]
    name = "bucket_and_prefix"
    prompt = "Bucket (and optional /prefix)"
    default = ""

  [[profiles.commands]]
  name = "gcloud-whoami"
  description = "Show the active gcloud account and project"
  command = "gcloud config list --format='value(core.account,core.project)'"

  [[profiles.commands]]
  name = "gcloud-ssh"
  description = "SSH into a GCE instance by name"
  command = "gcloud compute ssh {{instance}} --zone={{zone}}"

    [[profiles.commands.params]]
    name = "instance"
    prompt = "Instance name"
    default = ""

    [[profiles.commands.params]]
    name = "zone"
    prompt = "Zone"
    default = "us-central1-a"
```

## Databases

```toml
  [[profiles.commands]]
  name = "psql-connect"
  description = "Open a psql session against a database"
  command = "psql -h {{host}} -U {{user}} -d {{database}}"

    [[profiles.commands.params]]
    name = "host"
    prompt = "Host"
    default = "localhost"

    [[profiles.commands.params]]
    name = "user"
    prompt = "User"
    default = "postgres"

    [[profiles.commands.params]]
    name = "database"
    prompt = "Database"
    default = "postgres"

  [[profiles.commands]]
  name = "pg-dump-db"
  description = "Dump a Postgres database to a timestamped local file"
  command = "pg_dump -h {{host}} -U {{user}} {{database}} > {{database}}-$(date +%Y%m%d%H%M%S).sql"

    [[profiles.commands.params]]
    name = "host"
    prompt = "Host"
    default = "localhost"

    [[profiles.commands.params]]
    name = "user"
    prompt = "User"
    default = "postgres"

    [[profiles.commands.params]]
    name = "database"
    prompt = "Database"
    default = ""

  [[profiles.commands]]
  name = "redis-cli-connect"
  description = "Open a redis-cli session against a host"
  command = "redis-cli -h {{host}} -p {{port}}"

    [[profiles.commands.params]]
    name = "host"
    prompt = "Host"
    default = "localhost"

    [[profiles.commands.params]]
    name = "port"
    prompt = "Port"
    default = "6379"

  [[profiles.commands]]
  name = "mysql-connect"
  description = "Open a mysql session against a database"
  command = "mysql -h {{host}} -u {{user}} -p {{database}}"

    [[profiles.commands.params]]
    name = "host"
    prompt = "Host"
    default = "localhost"

    [[profiles.commands.params]]
    name = "user"
    prompt = "User"
    default = "root"

    [[profiles.commands.params]]
    name = "database"
    prompt = "Database"
    default = ""

  [[profiles.commands]]
  name = "mongo-connect"
  description = "Open a mongosh session against a URI"
  command = "mongosh \"{{uri}}\""

    [[profiles.commands.params]]
    name = "uri"
    prompt = "Connection URI"
    default = "mongodb://localhost:27017"
```

## Security & crypto

```toml
  [[profiles.commands]]
  name = "cert-expiry"
  description = "Show the expiry date of a remote site's TLS certificate"
  command = "echo | openssl s_client -servername {{domain}} -connect {{domain}}:443 2>/dev/null | openssl x509 -noout -enddate"

    [[profiles.commands.params]]
    name = "domain"
    prompt = "Domain"
    default = ""

  [[profiles.commands]]
  name = "gen-random-password"
  description = "Generate a random password of a given length"
  command = "openssl rand -base64 {{length}} | tr -d '=+/' | cut -c1-{{length}}"

    [[profiles.commands.params]]
    name = "length"
    prompt = "Length"
    default = "24"

  [[profiles.commands]]
  name = "sha256-file"
  description = "Compute the SHA-256 checksum of a file"
  command = "shasum -a 256 {{path}} || sha256sum {{path}}"

    [[profiles.commands.params]]
    name = "path"
    prompt = "File path"
    default = ""

  [[profiles.commands]]
  name = "decode-jwt"
  description = "Decode a JWT's header and payload without verifying the signature"
  command = "echo {{token}} | cut -d. -f1 | base64 -d 2>/dev/null; echo; echo {{token}} | cut -d. -f2 | base64 -d 2>/dev/null; echo"

    [[profiles.commands.params]]
    name = "token"
    prompt = "JWT"
    default = ""
    secret = true

  [[profiles.commands]]
  name = "gpg-encrypt-file"
  description = "Encrypt a file for a recipient's public key"
  command = "gpg --encrypt --recipient {{recipient}} {{path}}"

    [[profiles.commands.params]]
    name = "recipient"
    prompt = "Recipient (key id or email)"
    default = ""

    [[profiles.commands.params]]
    name = "path"
    prompt = "File to encrypt"
    default = ""

  [[profiles.commands]]
  name = "scan-local-ports"
  description = "Scan a host for open ports in the common range"
  command = "nmap -p 1-1024 {{host}}"

    [[profiles.commands.params]]
    name = "host"
    prompt = "Host to scan"
    default = "localhost"
```
