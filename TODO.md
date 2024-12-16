say in xmlhub repo README
- that key is case insensitive
- that string lists are split on " "
- that there is one key: val per comment only! and that there is no escaping or so afterars. and that's why
- the special string "NA" means the same as ""
- whitespace is normalized, hence choice of where to place newlines or indentation doesn't matter

Other

- include version number from git describe? have a publish script or nah, sgh, depend on git where build. (oh fun, people having outdated versions, what to do, version number in hub repo??)

- static linking

- cross compilation Mac OS (Windows?)

- remove the "resources" symlink after ahtml is proper

