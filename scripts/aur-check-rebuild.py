#!/usr/bin/env python

# pylint: disable=bare-except, missing-module-docstring, invalid-name, no-name-in-module, redefined-outer-name, missing-class-docstring, missing-function-docstring


from multiprocessing import Pool
from pathlib import Path
import logging
import os
import re
import shutil
import subprocess
import sys
import threading
import time

import helpers.database as database
import helpers.libalpm as libalpm
from helpers.settings import Settings

ANSI_ESCAPE = re.compile(r"\x1b\[[0-9;]*m")
IN_REBUILD_FILE = "/tmp/in_rebuild"
SO_PATTERN = re.compile(r"lib[^/]+\.so(\.[0-9]+)*$")
DB_LOCK_FILE = "/var/lib/pacman/db.lck"
DEP_CACHE = {}


class StripColorFormatter(logging.Formatter):
    def format(self, record):
        message = super().format(record)
        return ANSI_ESCAPE.sub("", message)


def __get_packages_from_stdin():
    local_packages: list[str] = []
    for target in sys.stdin:
        local_packages.append(target.strip())
    return local_packages


def __detect_aur_helper():
    if shutil.which("paru"):
        return "paru"

    if shutil.which("yay"):
        return "yay"

    logging.error("No AUR helper found")
    sys.exit(1)


def __initialize():
    settings = Settings.load()

    logger = logging.getLogger()
    logger.setLevel(logging.getLevelName(logging.DEBUG))

    logger.handlers.clear()
    console_handler = logging.StreamHandler()
    console_handler.setLevel(settings.log.level)
    console_handler.setFormatter(logging.Formatter("  %(message)s"))
    logger.addHandler(console_handler)

    helper = __detect_aur_helper()

    return settings, helper


def __handle_package_file(file):
    file = "/" + file
    is_so = SO_PATTERN.search(file) is not None
    required = set()

    try:
        output = subprocess.check_output(
            ["ldd", file], text=True, stderr=subprocess.DEVNULL
        )

        for line in output.splitlines():
            if "=>" not in line:
                continue

            right = line.partition("=>")[2].strip()
            so = right.split(" (", 1)[0]
            required.add(so)

    except:
        pass

    return (file, is_so, required)


def __handle_package_install(pkg: str, pool, cursor):
    def normalize_dep(dep: str) -> str:
        return re.split(r"[<>=]", dep, maxsplit=1)[0]

    libalpm_client = libalpm.get_libalpm()
    lib_pkg = libalpm_client.get_localdb().get_pkg(pkg)
    if not lib_pkg:
        return

    so_files = set()
    dep_files = set()
    for res in pool.imap_unordered(
        __handle_package_file,
        (f[0] for f in lib_pkg.files),
    ):
        if res[1]:
            so_files.add(res[0])
        dep_files.update(res[2])

    depends = [normalize_dep(dep) for dep in lib_pkg.depends]
    depends = [dep for dep in depends if dep in libalpm.get_all_pkgs(libalpm_client)]

    database.register_package(
        cursor,
        pkg,
        pkg in libalpm.build_aur_set(libalpm_client),
        depends,
        so_files,
        dep_files,
    )


def __gen_database():
    print("Creating package cache...")
    if os.path.exists(database.DATABASE_FILE):
        os.unlink(database.DATABASE_FILE)

    conn, cursor = database.open_connection()
    database.create_schema(cursor)
    libalpm_client = libalpm.get_libalpm()
    all_pkgs = libalpm.get_all_pkgs(libalpm_client)
    with Pool() as pool:
        for pkg in all_pkgs:
            __handle_package_install(pkg, pool, cursor)
    print("Writing database...")
    conn.commit()


def __register_new_packages():
    __initialize()
    packages = __get_packages_from_stdin()
    if not packages:
        return

    conn, cursor = database.open_connection()

    logging.info("Updating database with %s new packages", len(packages))
    with Pool() as pool:
        for pkg in packages:
            logging.info("  -> %s", pkg)
            __handle_package_install(pkg, pool, cursor)
    conn.commit()


def __unregister_packages():
    __initialize()
    packages = __get_packages_from_stdin()
    if not packages:
        return

    conn, cursor = database.open_connection()

    logging.info("Updating database deleting %s packages", len(packages))
    for pkg in packages:
        logging.info("  -> %s", pkg)
        database.delete_package(cursor, pkg)
    conn.commit()


def __on_upgrade():
    settings, helper = __initialize()
    cursor = database.open_connection()[1]
    packages = __get_packages_from_stdin()

    rebuild_dependencies = {}
    while packages:
        new_added = []
        for entry in database.get_aur_packages_so_depends(packages, cursor):
            if entry[0] not in rebuild_dependencies:
                rebuild_dependencies[entry[0]] = []
                new_added.append(entry[0])
            rebuild_dependencies[entry[0]].append(entry[1])
        packages = new_added

    pkgs = sorted(rebuild_dependencies.keys())
    logging.info(
        "Packages to rebuild: %s",
        len(pkgs),
    )
    if rebuild_dependencies:
        for package in sorted(rebuild_dependencies):
            logging.info(
                "  \033[1;37m%s\033[0m: \033[90m%s\033[0m",
                package,
                ", ".join(sorted(rebuild_dependencies[package])),
            )

        if settings.rebuild.automatic:
            db_lock_exists = False
            if os.path.exists(DB_LOCK_FILE):
                os.unlink(DB_LOCK_FILE)
                db_lock_exists = True

            with open(IN_REBUILD_FILE, "w", encoding="utf-8") as file:
                file.write(str(os.getpid()))

            try:
                command = (
                    f"{helper} -S --aur "
                    '--mflags "--skippgpcheck --skipchecksums --nocheck" '
                    "--sudoloop --noconfirm "
                    f'{" ".join(pkgs)}'
                )

                user = os.environ.get("SUDO_USER", "root")
                cmd = [
                    "alacritty",
                    "-o",
                    "window.opacity=1.0",
                    "-T",
                    "Rebuilding AUR packages",
                    "-e",
                    "bash",
                    "-c",
                    (
                        f'echo "Rebuilding packages:\n  {"\n  ".join(pkgs)}\n" '
                        f'&& {command}; read -p "Press enter to exit..." exit $?'
                    ),
                ]
                if os.geteuid() == 0:
                    cmd = [
                        "sudo",
                        "-u",
                        user,
                        "--",
                    ] + cmd

                stop_event = threading.Event()
                finished = []
                pre_rebuild_time = int(time.time())

                def check_fn():
                    while not stop_event.is_set():
                        while os.path.exists(DB_LOCK_FILE):
                            time.sleep(0.1)
                        db = libalpm.get_libalpm().get_localdb()
                        for p in [pcks for pcks in pkgs if pcks not in finished]:
                            pkg = db.get_pkg(p)
                            if not pkg:
                                continue
                            if pre_rebuild_time < pkg.installdate:
                                finished.append(p)
                                logging.info(
                                    "  [%s/%s] Rebuilt %s", len(finished), len(pkgs), p
                                )
                                continue
                        stop_event.wait(0.5)

                thread = threading.Thread(target=check_fn, args=(), daemon=True)
                thread.start()

                logging.info("Waiting for rebuild process...")
                tp0 = time.time()
                proc = subprocess.run(
                    cmd,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    stdin=subprocess.DEVNULL,
                    start_new_session=True,
                    check=False,
                )

                logging.debug(
                    "Finished after %s with status %s",
                    int((time.time() - tp0) * 1000) / 1000,
                    proc.returncode,
                )
            finally:
                stop_event.set()
                thread.join(timeout=1)

                Path(IN_REBUILD_FILE).unlink()
                if db_lock_exists:
                    lck = Path(DB_LOCK_FILE)
                    lck.touch()
                    os.chmod(lck, 0o000)

                pending = [pcks for pcks in pkgs if pcks not in finished]
                if not pending:
                    logging.info("  Packages rebuilt succesfully")
                else:
                    logging.info(
                        "  Process finished with %s pending packages", len(pending)
                    )
                    logging.info("    Run the following command to perform rebuild:")
                    logging.info("      %s -S --aur %s", helper, " ".join(pending))
        else:
            logging.info("Run the following command to perform rebuild:")
            logging.info("  %s -S --aur %s", helper, " ".join(pkgs))


if __name__ == "__main__":
    if "--database" in sys.argv:
        __gen_database()
    elif "--install" in sys.argv:
        __register_new_packages()
    elif "--uninstall" in sys.argv:
        __unregister_packages()
    elif "--upgrade" in sys.argv:
        __on_upgrade()
