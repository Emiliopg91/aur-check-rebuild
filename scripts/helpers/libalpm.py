# pylint: disable=missing-module-docstring, missing-function-docstring

from multiprocessing import Pool
from functools import cache

import logging
import os
import re
import shutil
import subprocess
import sys

SO_PATTERN = re.compile(r"lib[^/]+\.so(\.[0-9]+)*$")
DB_LOCK_FILE = "/var/lib/pacman/db.lck"
PACMAN_CONF = "/etc/pacman.conf"


def get_packages_with_so(allpkgs, upd_pkgs):
    pkgs = {}

    logging.debug("    Getting packages with .SO...")

    for name in upd_pkgs:
        pkg = allpkgs.get(name)
        if pkg is None:
            continue

        matched_files = []
        for file_path, *_ in pkg.files:
            if SO_PATTERN.search(file_path):
                matched_files.append("/" + file_path)

        if matched_files:
            pkgs[name] = matched_files
            logging.debug("      %s: %s", name, matched_files)

    return pkgs


def build_aur_set(local_db, sync_dbs):
    sync_pkg_names = {p.name for db in sync_dbs for p in db.pkgcache}

    return {
        pkg.name: (pkg.depends, [f[0] for f in pkg.files])
        for pkg in local_db.pkgcache
        if pkg.name not in sync_pkg_names
    }


def filter_packages_from_aur(aurpkgs, deps):
    aur_pkgs_with_dep = {}
    logging.debug("    Getting AUR dependant packages...")
    for pkg, (depends, files) in aurpkgs.items():
        if any(dep in deps for dep in depends):
            aur_pkgs_with_dep[pkg] = (depends, files)
            logging.debug(
                "      %s: %s", pkg, " ".join([dep for dep in deps if dep in depends])
            )

    return aur_pkgs_with_dep


@cache
def __process_file(file):
    required = set()

    try:
        output = subprocess.check_output(
            ["ldd", file],
            text=True,
            stderr=subprocess.DEVNULL,
        )

        for line in output.splitlines():
            if "=>" not in line:
                continue

            right = line.partition("=>")[2].strip()
            so = right.split(" (", 1)[0]
            required.add(so)

    except subprocess.CalledProcessError:
        pass

    return required


def get_dependant_packages(aurpkgs, so_packages):
    results = {}

    with Pool() as pool:
        for pkg, (depends, files) in aurpkgs.items():
            required_so = set()

            valid_files = ("/" + f for f in files if os.path.isfile("/" + f))

            for res in pool.imap_unordered(__process_file, valid_files):
                required_so.update(res)

            if required_so:
                for so_pkg, so_files in so_packages.items():
                    if so_pkg in depends and any(so in so_files for so in required_so):
                        results.setdefault(pkg, set()).add(so_pkg)

    return results


def removed_undependant_updated_packages(upd_pkgs, aurpkgs):
    logging.debug("    Cleaning packages without dependants...")
    res = [
        filtered
        for filtered in upd_pkgs
        if any(
            aurpkg for aurpkg, (depends, *_) in aurpkgs.items() if filtered in depends
        )
    ]
    for r in res:
        logging.debug("      %s", r)
    return res


def detect_aur_helper():
    if shutil.which("paru"):
        return "paru"

    if shutil.which("yay"):
        return "yay"

    logging.error("No AUR helper found")
    sys.exit(1)
