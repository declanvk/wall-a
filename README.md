# WALL•A

The `wall-a` CLI tool is intended to support writing data into a binary format
in the context of a git repository.

## Motivation

My initial idea was writing benchmark/profiling data to the git repository,
and storing it in a format that would not cause issues for git (_mostly_).

I wanted to write 1 piece of data per commit, and then wanted some way to get
the aggregate data out so that I could maybe visualize or otherwise use the
benchmark data.

## Design

The tool has two commands:
 - `append` - this command will read JSON data from STDIN and append it to a staging
   file in a specified "data" directory. If the staging file grows too large,
   then the contents of the staging file are read, merged together, and then written
   as in a binary format (CBOR) to a new "archive" file. The archive file has a
   timestamp as part of the filename, so it is ordered with respect to all previous
   archive files.
 - `read` - this command reads all the archive files in order by filename, merges
   the values each contains, then reads and merges the staging file values as well.
   Then it takes the final value and writes it to standard output.

Important to note that the JSON data written by `append` is merged with all previous
data when it is `read`. The merge function works like:
 - For a pair of JSON objects, it recursive merges common keys, otherwise it just takes
   the values for non-common keys. For example, merging
   `{"key": "value1", "some":"other"}` and `{"key": "value2", "un":"related"}` gives
   `{"key": "value2", "some":"other", "un":"related"}`.
 - For a pair of JSON arrays, it concatenates the new value after the old one. For
   example, merging `[1, 2, 3]` and `[4, 5, 6]` gives `[1, 2, 3, 4, 5, 6]`.
 - For all other combinations, it always takes the newer JSON value

The design is somewhat inspired by https://simonwillison.net/2020/Oct/9/git-scraping/,
I wanted to have `git diff` work for the most recent data. However, I didn't want there
to be a huge JSONL file that grew without bound, so as a compromise I added the
idea of the "archive" file. 

The "archive" file is just a snapshot of the staging file data, converted to a binary
format. This binary file can be much smaller and faster to read than the staging file.
The downside is that this file is in binary and doesn't interact with git well. The
archive file are only written 1 time, to reduce the number of copies of the file
git needs to store in the history.

The staging file is just a newline-delimited JSON file (JSONL). This format is great
for `git diff`, since you can easily see the newly added data and the data which was
transferred to the archive file.
