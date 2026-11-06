package dev.brahma.samriddhi.device

import androidx.core.content.FileProvider

// Its own class, so the manifest merger keeps it apart from the app's own FileProvider.
class UpdatesFileProvider : FileProvider()
