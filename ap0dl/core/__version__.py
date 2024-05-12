import importlib.metadata

try:
    __core__ = importlib.metadata.version("ap0dl")
except importlib.metadata.PackageNotFoundError:
    import pathlib

    from ap0dl.utils.optopt import regexlib

    __core__ = regexlib.search(
        r'name = "ap0dl"\nversion = "(.+?)"',
        (pathlib.Path(__file__).parent.parent / "pyproject.toml").read_text(),
    ).group(1)
