# Python scripts to blind sequence data in BEAST2 files

<span style="color: red">**Note**</span>: the [xmlhub tool](tool.html) has this functionality built in (in its `add-to` and `prepare` subcommands), so these scripts are purely in case you don't want to use that tool for some reason.

These scripts will replace the genetic sequences with a dummy all-gap alignment while keeping everything else (e.g. tip-dates) intact so the XML file still runs. If your XML file contains other non-sequence private data these scripts are not applicable! Keep in mind that `beast2blinder.py` and `beast1blinder.py` were written for BEAST 2.6 and BEAST 1.10, respectively, and have not been tested on other versions!

* [beast2blinder.py](beast2blinder.py) for BEAST 2.6

* [beast1blinder.py](beast1blinder.py) for BEAST 1.10 (currently not relevant to XML Hub, as XML Hub only cares about BEAST 2 files)

