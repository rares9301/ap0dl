package com.ap0dl.anyomi.ui.screens

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ap0dl.anyomi.data.Api
import com.ap0dl.anyomi.data.Loadable
import com.ap0dl.anyomi.data.SearchHit
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class BrowseViewModel : ViewModel() {
    private val _query = MutableStateFlow("")
    val query: StateFlow<String> = _query.asStateFlow()

    private val _provider = MutableStateFlow<String?>(null)
    val provider: StateFlow<String?> = _provider.asStateFlow()

    private val _results = MutableStateFlow<Loadable<List<SearchHit>>>(Loadable.Idle)
    val results: StateFlow<Loadable<List<SearchHit>>> = _results.asStateFlow()

    private var debounce: Job? = null
    private var request: Job? = null

    fun onQueryChange(value: String) {
        _query.value = value
        debounce?.cancel()
        if (value.trim().isEmpty()) {
            request?.cancel()
            if (_provider.value == null) {
                _results.value = Loadable.Idle
            } else {
                loadLatest(_provider.value!!)
            }
            return
        }
        if (value.trim().length < 2) {
            request?.cancel()
            _results.value = Loadable.Idle
            return
        }
        debounce = viewModelScope.launch {
            delay(650)
            searchNow()
        }
    }

    fun submitQuery() {
        debounce?.cancel()
        if (_query.value.trim().length >= 2) searchNow()
    }

    fun suggest(value: String) {
        _query.value = value
        debounce?.cancel()
        searchNow()
    }

    fun selectProvider(id: String?) {
        if (_provider.value == id) return
        _provider.value = id
        request?.cancel()
        if (_query.value.isBlank()) {
            if (id == null) {
                _results.value = Loadable.Idle
            } else {
                loadLatest(id)
            }
        } else if (_query.value.trim().length >= 2) {
            searchNow()
        }
    }

    fun applyNavProvider(id: String?) {
        if (id.isNullOrBlank() || id == "all") return
        selectProvider(id)
    }

    fun searchNow() {
        val query = _query.value.trim()
        if (query.length < 2) return
        request?.cancel()
        val provider = _provider.value
        request = viewModelScope.launch {
            _results.value = Loadable.Loading
            _results.value = try {
                Loadable.Ready(Api.search(query, provider, 36))
            } catch (error: Exception) {
                Loadable.Failed(error.message ?: "Search failed")
            }
        }
    }

    fun loadLatest(provider: String) {
        request?.cancel()
        request = viewModelScope.launch {
            _results.value = Loadable.Loading
            _results.value = try {
                Loadable.Ready(Api.latest(provider))
            } catch (error: Exception) {
                Loadable.Failed(error.message ?: "Could not load ${provider}")
            }
        }
    }

    fun retry() {
        val provider = _provider.value
        if (_query.value.isBlank()) {
            if (provider != null) loadLatest(provider) else _results.value = Loadable.Idle
        } else {
            searchNow()
        }
    }
}