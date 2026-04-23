# pylint: disable=missing-class-docstring, missing-module-docstring, missing-function-docstring

from dataclasses import dataclass, field
from dataclasses_json import DataClassJsonMixin

SETTINGS_FILE = "/usr/share/aur-check-rebuild/settings.json"


@dataclass
class ScanSettings(DataClassJsonMixin):
    recursive: bool = field(default=True)


@dataclass
class RebuildSettings(DataClassJsonMixin):
    automatic: bool = field(default=True)


@dataclass
class LogSettings(DataClassJsonMixin):
    level: str = field(default="INFO")
    path: str = field(default=None)


@dataclass
class Settings(DataClassJsonMixin):
    scan: ScanSettings = field(default_factory=ScanSettings)
    rebuild: RebuildSettings = field(default_factory=RebuildSettings)
    log: LogSettings = field(default_factory=LogSettings)

    @staticmethod
    def load(path=SETTINGS_FILE):
        with open(path, "r", encoding="utf-8") as f:
            settings = Settings.from_json(f.read())
        return settings
