# 🐑 polymorph: transform Blizzard CDN files into compact, binary archives

For [noclip.website](https://github.com/magcius/noclip.website)'s World of Warcraft renderer, we needed a tiny, fast file format for accessing millions of game files by either their ID or name in the browser. This isn't easily done without access to [CASC](https://wowdev.wiki/CASC) files, which Blizzard's installer creates dynamically on game installation, or without querying hundreds of megabytes of [TACT](https://wowdev.wiki/TACT) index files from Blizzard's CDN.

Enter polymorph, a tool which pulls all of a game's files from Blizzard's CDN and archives them into a `sheepfile` directory. `sheepfile`s consist of an tiny index file and several binary `.shp` archives -- load the index, and you can easily find a file by either its name or ID, and pull only the bytes you need from its respective `.shp` archive.
