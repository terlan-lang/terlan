module phase_constructor_chain_resolution.

pub constructor User {
    (id: Int, name: Binary): Dynamic ->
        id
}.

pub build(id: Int, name: Binary): Dynamic ->
    User(id, name) with Admin { id = id, name = name }.
