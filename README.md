# zsh

## zsh Installation
[zshを使ってみる - Qiita](https://qiita.com/ryutoyasugi/items/cb895814d4149ca44f12)

## Settings

```
~/
 ├ .zsh/
 │ └ .zshrc
 └ .zshenv
```

To read `~/.zsh/.zshrc`, write this in `~/.zshenv`:

```
export ZDOTDIR=$HOME/.zsh
```

## Requirements
- zsh-completions

```
brew install zsh-completions
```
