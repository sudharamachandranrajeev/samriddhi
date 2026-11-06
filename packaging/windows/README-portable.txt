Samriddhi - portable version

Run samriddhi.exe. Nothing is installed.

Your data is kept in your Windows profile (%APPDATA%\dev.brahma.samriddhi),
not in this folder, so replacing these files does not touch it.

This version cannot update itself. When Samriddhi tells you a new version
is out, download the new portable zip from the release page and replace
the files in this folder with the new ones.

Windows may warn that the program is from an unknown publisher: choose
More info, then Run anyway. To check the download, compare
  certutil -hashfile Samriddhi_<version>_x64_portable.zip SHA256
with the line in SHA256SUMS.txt on the release page.

Needs Windows 10 or 11 (with the WebView2 runtime, included in both).
