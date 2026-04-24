#!/usr/bin/env python

# pylint: disable=bare-except, missing-module-docstring, invalid-name, no-name-in-module, redefined-outer-name, missing-class-docstring, missing-function-docstring


from pathlib import Path
import logging
import os
import re
import subprocess
import sys
import threading
import time


from pycman.config import init_with_config
import psutil

from helpers.settings import Settings, ScanSettings, RebuildSettings
from helpers import libalpm

IN_REBUILD_FILE = "/tmp/in_rebuild"


def __get_updated_packages():
    local_packages: list[str] = []
    for target in sys.stdin:
        local_packages.append(target.strip())
    return local_packages


def __launch_rebuild_cmd(pkgs, prt, helper, rebuild_settings: RebuildSettings):
    if rebuild_settings.automatic:
        db_lock_exists = False
        if os.path.exists(libalpm.DB_LOCK_FILE):
            os.unlink(libalpm.DB_LOCK_FILE)
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

            def check_fn():
                while not stop_event.is_set():
                    while os.path.exists(libalpm.DB_LOCK_FILE):
                        time.sleep(0.1)
                    db = init_with_config(libalpm.PACMAN_CONF).get_localdb()
                    for p in [pcks for pcks in pkgs if pcks not in finished]:
                        pkg = db.get_pkg(p)
                        if not pkg:
                            continue
                        if prt < pkg.installdate:
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
                lck = Path(libalpm.DB_LOCK_FILE)
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


def __get_packages_to_rebuild(allpkgs, aurpkgs, scan_settings: ScanSettings):
    updated_packages = __get_updated_packages()
    logging.info("Triggered by %s packages", len(updated_packages))
    for p in updated_packages:
        logging.debug("  %s", p)

    logging.info("Found %s pacman packages", len(allpkgs))
    logging.info("Found %s AUR packages...", len(aurpkgs))
    for p in aurpkgs:
        logging.debug("  %s", p)

    logging.info("Looking for dependant packages...")
    ptr = []
    pdm = {}
    iteration = 0
    while scan_settings.recursive or iteration == 0:
        logging.debug("  Iteration %s", iteration)

        updated_packages = libalpm.removed_undependant_updated_packages(
            updated_packages, aurpkgs
        )

        packages_with_so = libalpm.get_packages_with_so(allpkgs, updated_packages)

        packages_from_aur = {
            p: (d, f)
            for p, (d, f) in libalpm.filter_packages_from_aur(
                aur_pkgs, packages_with_so
            ).items()
            if p not in ptr
        }

        tmp_list = {
            p: d
            for p, d in libalpm.get_dependant_packages(
                packages_from_aur, packages_with_so
            ).items()
            if p not in updated_packages
        }

        if not tmp_list:
            break

        new_updated_packages = []
        for p in tmp_list:
            ptr.append(p)
            pdm[p] = tmp_list[p]
            new_updated_packages.append(p)

        updated_packages = new_updated_packages

        iteration = iteration + 1

    return (ptr, pdm)


def __initialize():
    settings = Settings.load()

    logger = logging.getLogger()
    logger.setLevel(logging.getLevelName(logging.DEBUG))

    logger.handlers.clear()

    if settings.log.path:

        class StripColorFormatter(logging.Formatter):
            ANSI_ESCAPE = re.compile(r"\x1b\[[0-9;]*m")

            def format(self, record):
                message = super().format(record)
                return self.ANSI_ESCAPE.sub("", message)

        try:
            file_handler = logging.FileHandler(settings.log.path, encoding="utf-8")
            file_handler.setFormatter(
                StripColorFormatter("[%(asctime)s] [%(levelname)-6s] %(message)s")
            )
            logger.addHandler(file_handler)
            logger.debug("###########################################################")
        except:
            pass

    console_handler = logging.StreamHandler()
    console_handler.setLevel(settings.log.level)
    console_handler.setFormatter(logging.Formatter("  %(message)s"))
    logger.addHandler(console_handler)

    helper = libalpm.detect_aur_helper()

    libalpm_inst = init_with_config(libalpm.PACMAN_CONF)
    all_pkgs = {pkg.name: pkg for pkg in libalpm_inst.get_localdb().pkgcache}
    aur_pkgs = libalpm.build_aur_set(
        libalpm_inst.get_localdb(), libalpm_inst.get_syncdbs()
    )

    return settings, helper, libalpm_inst, all_pkgs, aur_pkgs


if __name__ == "__main__":
    if os.path.exists(IN_REBUILD_FILE):
        with open(IN_REBUILD_FILE, "r", encoding="utf-8") as f:
            pid = int(f.read())
        try:
            if __file__ in psutil.Process(pid).cmdline():
                logging.debug("Skipping due to rebuild...")
                sys.exit(0)
        except SystemExit:
            raise
        except:
            pass
    try:
        settings, helper, libalpm_inst, all_pkgs, aur_pkgs = __initialize()

        t0 = time.time()

        packages_to_rebuild, packages_dep_map = __get_packages_to_rebuild(
            all_pkgs, aur_pkgs, settings.scan
        )
        packages_to_rebuild = sorted(packages_to_rebuild)

        t1 = time.time()

        logging.info(
            "  Scan finished after %s seconds",
            int((t1 - t0) * 1000) / 1000,
        )
        logging.info(
            "Packages to rebuild: %s",
            len(packages_to_rebuild),
        )
        if len(packages_to_rebuild) > 0:
            for package in packages_to_rebuild:
                logging.info(
                    "  \033[1;37m%s\033[0m: \033[90m%s\033[0m",
                    package,
                    ", ".join(sorted(packages_dep_map[package])),
                )

            pre_rebuild_time = int(time.time())
            __launch_rebuild_cmd(
                packages_to_rebuild, pre_rebuild_time, helper, settings.rebuild
            )
    except subprocess.CalledProcessError as e:
        sys.exit(e.returncode)
