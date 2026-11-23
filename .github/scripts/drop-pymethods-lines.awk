# Drops lcov line records for `#[pymethods]` attribute lines. LLVM attributes
# PyO3's generated glue to the attribute line and never counts it as run, so
# every such line reads as uncovered although its methods are covered on their
# own lines. Codecov derives line coverage from DA records, so LF/LH are left.
# Usage: awk -F'[:,]' -f drop-pymethods-lines.awk rust.lcov
/^SF:/ {
    delete source
    file = substr($0, 4)
    count = 0
    while ((getline line < file) > 0) source[++count] = line
    close(file)
}
/^DA:/ {
    text = source[$2]
    gsub(/^[ \t]+|[ \t]+$/, "", text)
    if (text == "#[pymethods]") next
}
{ print }
