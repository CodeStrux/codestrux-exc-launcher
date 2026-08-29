#!/bin/bash
# Legacy reference implementation: a plain-bash `select` menu using only
# common, generic example commands. This is the shape of script that
# inspired `exc` (see the Rust source in src/ and the README), kept here
# as a minimal, dependency-free example you can copy and extend directly
# if you'd rather not use the compiled binary at all.
#
# Usage:
#   ./launcher.sh          interactive menu
#   ./launcher.sh 3        run option #3 directly

clear
PS3="Opt: "
COLUMNS=$(tput cols)
options=(
    "disk-usage"
    "top-processes"
    "print-hosts"
    "git-status"
    "git-log-graph"
    "git-clean-branches"
    "docker-ps"
    "docker-prune"
    "ping-host"
    "cert-check-online"
    "dns-lookup"
    "gen-password"
    "exit"
);

execOption () {
  echo "Running: $1"
  case $1 in
      "disk-usage")          du -sh .;;
      "top-processes")       ps aux | sort -rk 3 | head -n 10;;
      "print-hosts")         cat /etc/hosts;;
      "git-status")          git status -sb;;
      "git-log-graph")       git log --oneline --graph --decorate -n 30;;
      "git-clean-branches")  git fetch --prune; git branch --merged main | grep -v '\*\|main';;
      "docker-ps")           docker ps;;
      "docker-prune")        docker container prune -f; docker image prune -f; docker volume prune -f;;
      "ping-host")           echo -n 'Host to ping: '; read host; ping -c 4 "$host";;
      "cert-check-online")   echo -n 'Domain to check: '; read domain; openssl s_client -connect "$domain":443 </dev/null 2>/dev/null | openssl x509 -noout -dates -subject;;
      "dns-lookup")          echo -n 'Domain to look up: '; read domain; dig +short "$domain" A AAAA MX;;
      "gen-password")        echo -n 'Password byte length [20]: '; read len; openssl rand -base64 "${len:-20}" | tr -d '\n'; echo;;
      "exit")                echo "Bye";;
      *)                     echo "invalid option";;
  esac
}

if [ -n "$1" ]; then
  if [[ "$1" =~ ^[0-9]+$ ]] && [ "$1" -ge 1 ] && [ "$1" -le "${#options[@]}" ]; then
    execOption "${options[$(($1-1))]}"
  else
    echo "Error: use a valid number (1-${#options[@]})"
    exit 1
  fi
else
  select opt in "${options[@]}"
  do
    if [[ -n "$opt" ]]; then
      execOption "$opt"
      break
    else
      echo "Invalid option"
    fi
  done
fi
