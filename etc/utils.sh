function print_error() {
  echo -e "\033[0;31mError: ${1}\033[0m"
}

function print_success() {
  echo -e "\033[0;32mSuccess: ${1}\033[0m"
}

function print_info() {
  echo -e "\033[0;34mInfo: ${1}\033[0m"
}

function command_exists() {
  command=${1}

  if type ${command}; then
    :
  else
    exit 1
  fi
}
