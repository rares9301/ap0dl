"""
ap0dl: Optional optimisations for the project.
"""

try:
    import regex as regexlib
except ImportError:
    import re as regexlib

try:
    import orjson as jsonlib

    dumps_function = jsonlib.dumps

    def patched_dumps(*args, **kwargs):
        return dumps_function(*args, **kwargs).decode("utf-8")

    jsonlib.dumps = patched_dumps

except ImportError:
    import json as jsonlib
