function is_exist() {
  type code >/dev/null 2>&1;
}

function is_batch() {
  [[ $- == *i* ]]
}

function print_error() {
  echo -e "\033[0;31mError: ${1}\033[0m"
}

function print_success() {
  echo -e "\033[0;32mSuccess: ${1}\033[0m"
}

function print_info() {
  echo -e "\033[0;34mInfo: ${1}\033[0m"
}
