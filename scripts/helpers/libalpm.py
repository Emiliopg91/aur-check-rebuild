from pycman.config import init_with_config

PACMAN_CONF = "/etc/pacman.conf"


def get_libalpm():
    return init_with_config(PACMAN_CONF)


def build_aur_set(libalpm):
    local_db = libalpm.get_localdb()
    sync_dbs = libalpm.get_syncdbs()

    sync_pkg_names = {p.name for db in sync_dbs for p in db.pkgcache}

    return {
        pkg.name: (pkg.depends, [f[0] for f in pkg.files])
        for pkg in local_db.pkgcache
        if pkg.name not in sync_pkg_names
    }


def get_all_pkgs(libalpm):
    return {pkg.name: pkg for pkg in libalpm.get_localdb().pkgcache}
