import os
import sqlite3


DATABASE_FILE = os.path.join(os.getcwd(), "database.db")


def open_connection() -> tuple[sqlite3.Connection, sqlite3.Cursor]:
    conn = sqlite3.connect(DATABASE_FILE)
    cursor = conn.cursor()

    return (conn, cursor)


def create_schema(cursor: sqlite3.Cursor):
    cursor.execute(
        """
    CREATE TABLE IF NOT EXISTS packages (
        name TEXT PRIMARY KEY,
        aur BOOLEAN
    )
    """
    )

    cursor.execute(
        """
    CREATE TABLE IF NOT EXISTS dependencies (
        package TEXT NOT NULL,
        depends TEXT NOT NULL,
        PRIMARY KEY (package, depends),
        FOREIGN KEY (package) REFERENCES packages(name) ON DELETE CASCADE,
        FOREIGN KEY (depends) REFERENCES packages(name) ON DELETE CASCADE
    )
    """
    )
    cursor.execute(
        """
    CREATE INDEX idx_dependencies_depends ON dependencies(depends);
    """
    )

    cursor.execute(
        """
    CREATE TABLE so_files (
        package TEXT NOT NULL,
        file TEXT NOT NULL,
        PRIMARY KEY (package, file),
        FOREIGN KEY (package) REFERENCES packages(name) ON DELETE CASCADE
    );
    """
    )
    cursor.execute(
        """
    CREATE INDEX idx_so_files_package ON so_files(package);
    """
    )

    cursor.execute(
        """
    CREATE TABLE so_dependencies (
        package TEXT NOT NULL,
        file TEXT NOT NULL,
        PRIMARY KEY (package, file),
        FOREIGN KEY (package) REFERENCES packages(name) ON DELETE CASCADE,
        FOREIGN KEY (file) REFERENCES so_files(file) ON DELETE CASCADE
    );
    """
    )
    cursor.execute(
        """
    CREATE INDEX idx_so_dependencies_file ON so_dependencies(file);
    """
    )


def register_package(
    cursor,
    pkg: str,
    aur: bool,
    depends: list[str],
    so_files: list[str],
    dep_files: list[str],
):
    cursor.execute(
        "INSERT INTO packages (name, aur) VALUES (?,?)",
        (
            pkg,
            aur,
        ),
    )

    for depends in set(depends):
        cursor.execute(
            "INSERT INTO dependencies (package, depends) values (?,?)",
            (
                pkg,
                depends,
            ),
        )

    for file in so_files:
        cursor.execute(
            "INSERT INTO so_files (package, file) values (?,?)",
            (
                pkg,
                file,
            ),
        )

    for file in set(dep_files):
        cursor.execute(
            "INSERT INTO so_dependencies (package, file) values (?,?)",
            (
                pkg,
                file,
            ),
        )


def delete_package(
    cursor,
    pkg: str,
):

    cursor.execute(
        "DELETE FROM so_dependencies WHERE package=?",
        (pkg,),
    )

    cursor.execute(
        """
        DELETE FROM so_dependencies
        WHERE file IN (
            SELECT file FROM so_files WHERE package = ?
        )
        """,
        (pkg,),
    )

    cursor.execute(
        "DELETE FROM so_files where package=?",
        (pkg,),
    )

    cursor.execute(
        "DELETE FROM dependencies WHERE package=? or depends=?",
        (
            pkg,
            pkg,
        ),
    )

    cursor.execute(
        "DELETE FROM packages WHERE name=?",
        (pkg,),
    )


def get_aur_packages_so_depends(updated_pkgs, cursor):
    if not updated_pkgs:
        return []

    placeholders = ",".join("?" for _ in updated_pkgs)

    query = f"""
        SELECT DISTINCT p.name, sf.package
        FROM packages p
        JOIN so_dependencies sd ON p.name = sd.package
        JOIN so_files sf ON sd.file = sf.file
        WHERE p.aur = 1
        AND sf.package IN ({placeholders})
        and p.name != sf.package
    """

    cursor.execute(query, updated_pkgs)

    return cursor.fetchall()
