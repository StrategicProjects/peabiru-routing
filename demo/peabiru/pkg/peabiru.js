/* @ts-self-types="./peabiru.d.ts" */

/**
 * A network built once and shared by every routing call.
 */
export class Network {
    static __wrap(ptr) {
        const obj = Object.create(Network.prototype);
        obj.__wbg_ptr = ptr;
        NetworkFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        NetworkFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_network_free(ptr, 0);
    }
    /**
     * Build a network from the bytes of a `.pbf` file and of GTFS feeds:
     * `feeds` is an array of `[name, Uint8Array]` pairs, loaded in that
     * order; `tif` the bytes of an elevation raster, applied with the
     * slope cost function `elevation` (`"TOBLER"`, `"MINETTI"` or
     * `"NONE"`).
     * @param {Uint8Array | number[]} pbf
     * @param {Array<any>} feeds
     * @param {Uint8Array | number[] | null | undefined} tif
     * @param {string} elevation
     * @returns {Network}
     */
    static build(pbf, feeds, tif, elevation) {
        let ptr0, len0, ptr1, len1, ptr2, len2;
        try {
            ptr0 = passArray8ToWasm0(pbf, wasm.__wbindgen_malloc);
            len0 = WASM_VECTOR_LEN;
            ptr1 = isLikeNone(tif) ? 0 : passArray8ToWasm0(tif, wasm.__wbindgen_malloc);
            len1 = WASM_VECTOR_LEN;
            ptr2 = passStringToWasm0(elevation, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            len2 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 1, 1); }
            if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 1, 1); }
            throw e;
        }
        const ret = wasm.network_build(ptr0, len0, feeds, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Network.__wrap(ret[0]);
    }
    /**
     * Warnings about the network build: one per feed built from although
     * it has GTFS errors of priority `HIGH` (never from JavaScript, where
     * such feeds are refused), then those about the elevation raster.
     * @returns {Array<any>}
     */
    build_warnings() {
        const ret = wasm.network_build_warnings(this.__wbg_ptr);
        return ret;
    }
    /**
     * The slope cost function in use: `"TOBLER"`, `"MINETTI"` or `"NONE"`.
     * @returns {string}
     */
    elevation() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.network_elevation(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * The feed ids, in loading order.
     * @returns {Array<any>}
     */
    feed_ids() {
        const ret = wasm.network_feed_ids(this.__wbg_ptr);
        return ret;
    }
    /**
     * Directed edges of the street network.
     * @returns {number}
     */
    n_edges() {
        const ret = wasm.network_n_edges(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Vertices of the street network.
     * @returns {number}
     */
    n_vertices() {
        const ret = wasm.network_n_vertices(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * `[services, services active on date]` (`date` as `yyyymmdd`).
     * @param {number} date
     * @returns {Uint32Array}
     */
    transit_availability(date) {
        const ret = wasm.network_transit_availability(this.__wbg_ptr, date);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * `[stops, patterns, services, frequencies (0/1)]`.
     * @returns {Uint32Array}
     */
    transit_sizes() {
        const ret = wasm.network_transit_sizes(this.__wbg_ptr);
        var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
}
if (Symbol.dispose) Network.prototype[Symbol.dispose] = Network.prototype.free;

/**
 * Accessibility values `[origin][opportunity][percentile][cutoff]` in one
 * Float64Array. `mode` is the street mode (of the access to public
 * transport when `transit_modes` is not empty); `opportunities` holds the
 * `n_opportunities` opportunity columns one after the other, each with
 * one value per destination.
 * @param {Network} network
 * @param {Float64Array | number[]} olat
 * @param {Float64Array | number[]} olon
 * @param {Float64Array | number[]} dlat
 * @param {Float64Array | number[]} dlon
 * @param {Float64Array | number[]} opportunities
 * @param {number} n_opportunities
 * @param {string} mode
 * @param {string} transit_modes
 * @param {string} egress_mode
 * @param {string} decay_function
 * @param {number} decay_value
 * @param {Int32Array | number[]} cutoffs
 * @param {Int32Array | number[]} percentiles
 * @param {number} date
 * @param {number} departure_time
 * @param {number} time_window
 * @param {number} max_walk_time
 * @param {number} max_bike_time
 * @param {number} max_car_time
 * @param {number} max_trip_duration
 * @param {number} walk_speed
 * @param {number} bike_speed
 * @param {number} max_lts
 * @param {number} max_rides
 * @param {number} draws_per_minute
 * @param {number | null | undefined} seed
 * @param {any} scenario
 * @param {any} fare
 * @param {number} max_fare
 * @returns {Float64Array}
 */
export function accessibility(network, olat, olon, dlat, dlon, opportunities, n_opportunities, mode, transit_modes, egress_mode, decay_function, decay_value, cutoffs, percentiles, date, departure_time, time_window, max_walk_time, max_bike_time, max_car_time, max_trip_duration, walk_speed, bike_speed, max_lts, max_rides, draws_per_minute, seed, scenario, fare, max_fare) {
    _assertClass(network, Network);
    const val12 = +n_opportunities;
    const val13 = +decay_value;
    const val14 = +date;
    const val15 = +departure_time;
    const val16 = +time_window;
    const val17 = +max_walk_time;
    const val18 = +max_bike_time;
    const val19 = +max_car_time;
    const val20 = +max_trip_duration;
    const val21 = +walk_speed;
    const val22 = +bike_speed;
    const val23 = +max_lts;
    const val24 = +max_rides;
    const val25 = +draws_per_minute;
    const val26 = +(isLikeNone(seed) ? Number.MAX_SAFE_INTEGER : (seed) >> 0);
    const val27 = +max_fare;
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10;
    try {
        ptr0 = passArrayF64ToWasm0(olat, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(olon, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passArrayF64ToWasm0(dlat, wasm.__wbindgen_malloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passArrayF64ToWasm0(dlon, wasm.__wbindgen_malloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passArrayF64ToWasm0(opportunities, wasm.__wbindgen_malloc);
        len4 = WASM_VECTOR_LEN;
        ptr5 = passStringToWasm0(mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len5 = WASM_VECTOR_LEN;
        ptr6 = passStringToWasm0(transit_modes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len6 = WASM_VECTOR_LEN;
        ptr7 = passStringToWasm0(egress_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len7 = WASM_VECTOR_LEN;
        ptr8 = passStringToWasm0(decay_function, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len8 = WASM_VECTOR_LEN;
        ptr9 = passArray32ToWasm0(cutoffs, wasm.__wbindgen_malloc);
        len9 = WASM_VECTOR_LEN;
        ptr10 = passArray32ToWasm0(percentiles, wasm.__wbindgen_malloc);
        len10 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr10) { wasm.__wbindgen_free(ptr10, len10 * 4, 4); }
        if (ptr9) { wasm.__wbindgen_free(ptr9, len9 * 4, 4); }
        if (ptr8) { wasm.__wbindgen_free(ptr8, len8 * 1, 1); }
        if (ptr7) { wasm.__wbindgen_free(ptr7, len7 * 1, 1); }
        if (ptr6) { wasm.__wbindgen_free(ptr6, len6 * 1, 1); }
        if (ptr5) { wasm.__wbindgen_free(ptr5, len5 * 1, 1); }
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 8, 8); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 8, 8); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 8, 8); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 8, 8); }
        throw e;
    }
    const ret = wasm.accessibility(network.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, val12, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, val13, ptr9, len9, ptr10, len10, val14, val15, val16, val17, val18, val19, val20, val21, val22, val23, val24, val25, val26, scenario, fare, val27);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Version of the Rust core this module was built from.
 * @returns {string}
 */
export function core_version() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.core_version();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Expanded (or, with `arrival`, arrival) travel-time matrix as an array
 * of columns: `from`, `to` (0-based Int32Array), `departure` (Int32Array,
 * seconds after midnight), `draw` (Int32Array), the times `access_time`,
 * `wait_time`, `ride_time`, `transfer_time`, `egress_time`, `total_time`
 * (Float64Array, NaN for NA), `n_rides` (Int32Array, `NA_I32`
 * for NA), and `routes` dictionary-encoded as `route_codes` (Int32Array,
 * -1 for NA) into the array of strings `route_levels`. For an arrival
 * search the parameters must already be the departure side (date and
 * time of `arrival - max_trip_duration`, `time_window =
 * max_trip_duration`), as `lib/index.js` sets them.
 * @param {Network} network
 * @param {Float64Array | number[]} olat
 * @param {Float64Array | number[]} olon
 * @param {Float64Array | number[]} dlat
 * @param {Float64Array | number[]} dlon
 * @param {string} transit_modes
 * @param {string} access_mode
 * @param {string} egress_mode
 * @param {number} bike_speed
 * @param {number} max_lts
 * @param {number} max_bike_time
 * @param {number} max_car_time
 * @param {boolean} arrival
 * @param {number} date
 * @param {number} departure_time
 * @param {number} time_window
 * @param {number} max_walk_time
 * @param {number} max_trip_duration
 * @param {number} walk_speed
 * @param {number} max_rides
 * @param {number} draws_per_minute
 * @param {number} seed
 * @param {any} scenario
 * @returns {Array<any>}
 */
export function expanded(network, olat, olon, dlat, dlon, transit_modes, access_mode, egress_mode, bike_speed, max_lts, max_bike_time, max_car_time, arrival, date, departure_time, time_window, max_walk_time, max_trip_duration, walk_speed, max_rides, draws_per_minute, seed, scenario) {
    _assertClass(network, Network);
    const val8 = +bike_speed;
    const val9 = +max_lts;
    const val10 = +max_bike_time;
    const val11 = +max_car_time;
    const val12 = +arrival;
    const val13 = +date;
    const val14 = +departure_time;
    const val15 = +time_window;
    const val16 = +max_walk_time;
    const val17 = +max_trip_duration;
    const val18 = +walk_speed;
    const val19 = +max_rides;
    const val20 = +draws_per_minute;
    const val21 = +seed;
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6;
    try {
        ptr0 = passArrayF64ToWasm0(olat, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(olon, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passArrayF64ToWasm0(dlat, wasm.__wbindgen_malloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passArrayF64ToWasm0(dlon, wasm.__wbindgen_malloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passStringToWasm0(transit_modes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len4 = WASM_VECTOR_LEN;
        ptr5 = passStringToWasm0(access_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len5 = WASM_VECTOR_LEN;
        ptr6 = passStringToWasm0(egress_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len6 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr6) { wasm.__wbindgen_free(ptr6, len6 * 1, 1); }
        if (ptr5) { wasm.__wbindgen_free(ptr5, len5 * 1, 1); }
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 1, 1); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 8, 8); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 8, 8); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 8, 8); }
        throw e;
    }
    const ret = wasm.expanded(network.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, val8, val9, val10, val11, val12, val13, val14, val15, val16, val17, val18, val19, val20, val21, scenario);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * The checks of a structure given for a network.
 * @param {Network} network
 * @param {any} fare
 */
export function fare_check(network, fare) {
    _assertClass(network, Network);
    const ret = wasm.fare_check(network.__wbg_ptr, fare);
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

/**
 * `readFareStructure()` of the bytes of a `.zip` file.
 * @param {Uint8Array | number[]} bytes
 * @param {string} encoding
 * @returns {any}
 */
export function fare_read(bytes, encoding) {
    let ptr0, len0, ptr1, len1;
    try {
        ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passStringToWasm0(encoding, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len1 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 1, 1); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 1, 1); }
        throw e;
    }
    const ret = wasm.fare_read(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * `setupFareStructure()`, as the flat object.
 * @param {Network} network
 * @param {number} base_fare
 * @param {string} by
 * @returns {any}
 */
export function fare_setup(network, base_fare, by) {
    _assertClass(network, Network);
    const val2 = +base_fare;
    const ptr0 = passStringToWasm0(by, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.fare_setup(network.__wbg_ptr, val2, ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * `writeFareStructure()`: the bytes of the `.zip` file.
 * @param {any} fare
 * @returns {Uint8Array}
 */
export function fare_write(fare) {
    const ret = wasm.fare_write(fare);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Snaps of `findSnap()`: `[snap_lat, snap_lon, distance]`
 * (Float64Array, NaN where not found) and `found` (Uint8Array, 0/1).
 * @param {Network} network
 * @param {Float64Array | number[]} lat
 * @param {Float64Array | number[]} lon
 * @param {number} radius
 * @param {string} mode
 * @returns {Array<any>}
 */
export function find_snap(network, lat, lon, radius, mode) {
    _assertClass(network, Network);
    const val4 = +radius;
    let ptr0, len0, ptr1, len1, ptr2, len2;
    try {
        ptr0 = passArrayF64ToWasm0(lat, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(lon, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passStringToWasm0(mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len2 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 1, 1); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 8, 8); }
        throw e;
    }
    const ret = wasm.find_snap(network.__wbg_ptr, ptr0, len0, ptr1, len1, val4, ptr2, len2);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * The two-step floating catchment area: `[index, ratio,
 * catchment_demand]` (see [`locate_facilities`] for the table).
 * @param {number} n_demand
 * @param {number} n_sites
 * @param {Int32Array | number[]} pair_demand
 * @param {Int32Array | number[]} pair_site
 * @param {Float64Array | number[]} pair_minutes
 * @param {Float64Array | number[]} demand
 * @param {Float64Array | number[]} supply
 * @param {number} cutoff
 * @param {string} decay_function
 * @param {number} decay_value
 * @returns {Array<any>}
 */
export function floating_catchment(n_demand, n_sites, pair_demand, pair_site, pair_minutes, demand, supply, cutoff, decay_function, decay_value) {
    const val7 = +n_demand;
    const val8 = +n_sites;
    const val9 = +cutoff;
    const val10 = +decay_value;
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5;
    try {
        ptr0 = passArray32ToWasm0(pair_demand, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArray32ToWasm0(pair_site, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passArrayF64ToWasm0(pair_minutes, wasm.__wbindgen_malloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passArrayF64ToWasm0(demand, wasm.__wbindgen_malloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passArrayF64ToWasm0(supply, wasm.__wbindgen_malloc);
        len4 = WASM_VECTOR_LEN;
        ptr5 = passStringToWasm0(decay_function, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len5 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr5) { wasm.__wbindgen_free(ptr5, len5 * 1, 1); }
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 8, 8); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 8, 8); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 8, 8); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 4, 4); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 4, 4); }
        throw e;
    }
    const ret = wasm.floating_catchment(val7, val8, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, val9, ptr5, len5, val10);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * The errors gtfs-lib reports for one feed (its name and the bytes of
 * its `.zip`), as the columns of `getGtfsErrors()`: `[file, line
 * (Int32Array), type, field, id, priority]` (arrays, null where missing).
 * @param {string} name
 * @param {Uint8Array | number[]} zip
 * @returns {Array<any>}
 */
export function gtfs_errors(name, zip) {
    let ptr0, len0, ptr1, len1;
    try {
        ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArray8ToWasm0(zip, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 1, 1); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 1, 1); }
        throw e;
    }
    const ret = wasm.gtfs_errors(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Line isochrones from a matrix of median minutes (`NA_I32` where not
 * reported), `n_origins` rows of `n_edges` values, origin-major: the rows
 * in order as `[origin, edge, travel_time]` (0-based Int32Arrays)
 * and `isochrone` (Float64Array).
 * @param {number} n_origins
 * @param {number} n_edges
 * @param {Int32Array | number[]} minutes
 * @param {Float64Array | number[]} cutoffs
 * @returns {Array<any>}
 */
export function isochrone_lines(n_origins, n_edges, minutes, cutoffs) {
    const val3 = +n_origins;
    const val4 = +n_edges;
    let ptr0, len0, ptr1, len1;
    try {
        ptr0 = passArray32ToWasm0(minutes, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(cutoffs, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 4, 4); }
        throw e;
    }
    const ret = wasm.isochrone_lines(val3, val4, ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Polygon isochrones as an array of columns the JavaScript side assembles
 * into GeoJSON: per row (origin, percentile, cutoff, in this order)
 * `origin` (0-based Int32Array), `isochrone` (Float64Array), `percentile`
 * and `n_polygons` (Int32Array); per polygon `n_rings`; per ring
 * `n_points` (Int32Array); per point `x` (longitude) and `y` (latitude),
 * Float64Array. `mode` is the street mode (of the access to public
 * transport when `transit_modes` is not empty); `max_trip_duration` is
 * the search limit (the largest cutoff plus 10).
 * @param {Network} network
 * @param {Float64Array | number[]} olat
 * @param {Float64Array | number[]} olon
 * @param {string} mode
 * @param {string} transit_modes
 * @param {string} egress_mode
 * @param {Float64Array | number[]} cutoffs
 * @param {number} zoom
 * @param {Int32Array | number[]} percentiles
 * @param {number} date
 * @param {number} departure_time
 * @param {number} time_window
 * @param {number} max_walk_time
 * @param {number} max_bike_time
 * @param {number} max_car_time
 * @param {number} max_trip_duration
 * @param {number} walk_speed
 * @param {number} bike_speed
 * @param {number} max_lts
 * @param {number} max_rides
 * @param {number} draws_per_minute
 * @param {number} seed
 * @returns {Array<any>}
 */
export function isochrone_polygons(network, olat, olon, mode, transit_modes, egress_mode, cutoffs, zoom, percentiles, date, departure_time, time_window, max_walk_time, max_bike_time, max_car_time, max_trip_duration, walk_speed, bike_speed, max_lts, max_rides, draws_per_minute, seed) {
    _assertClass(network, Network);
    const val8 = +zoom;
    const val9 = +date;
    const val10 = +departure_time;
    const val11 = +time_window;
    const val12 = +max_walk_time;
    const val13 = +max_bike_time;
    const val14 = +max_car_time;
    const val15 = +max_trip_duration;
    const val16 = +walk_speed;
    const val17 = +bike_speed;
    const val18 = +max_lts;
    const val19 = +max_rides;
    const val20 = +draws_per_minute;
    const val21 = +seed;
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6;
    try {
        ptr0 = passArrayF64ToWasm0(olat, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(olon, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passStringToWasm0(mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passStringToWasm0(transit_modes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passStringToWasm0(egress_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len4 = WASM_VECTOR_LEN;
        ptr5 = passArrayF64ToWasm0(cutoffs, wasm.__wbindgen_malloc);
        len5 = WASM_VECTOR_LEN;
        ptr6 = passArray32ToWasm0(percentiles, wasm.__wbindgen_malloc);
        len6 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr6) { wasm.__wbindgen_free(ptr6, len6 * 4, 4); }
        if (ptr5) { wasm.__wbindgen_free(ptr5, len5 * 8, 8); }
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 1, 1); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 1, 1); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 1, 1); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 8, 8); }
        throw e;
    }
    const ret = wasm.isochrone_polygons(network.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, val8, ptr6, len6, val9, val10, val11, val12, val13, val14, val15, val16, val17, val18, val19, val20, val21);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Itineraries between `o[i]` and `d[i]` as an array of columns: `pair`
 * (0-based), `option`, `total_distance`, `segment`, `distance`,
 * `n_points` (Int32Array), `total_duration`, `segment_duration`, `wait`,
 * `x` (longitude), `y` (latitude) (Float64Array), `departure_time`,
 * `mode`, `route` (arrays of strings). The lines' points are `x`, `y`,
 * leg after leg, `n_points` per leg. With `osm_link_ids`, four more
 * arrays of strings: `osm_id_list`, `edge_id_list`, `board_stop_id`,
 * `alight_stop_id` (empty without), then `total_fare` and
 * `cumulative_fare` (Float64Array), and last the pairs (0-based,
 * Int32Array) whose itineraries in R5 depend on what ran before (see
 * [`itinerary_warnings`]).
 * @param {Network} network
 * @param {Float64Array | number[]} olat
 * @param {Float64Array | number[]} olon
 * @param {Float64Array | number[]} dlat
 * @param {Float64Array | number[]} dlon
 * @param {string} mode
 * @param {string} transit_modes
 * @param {string} egress_mode
 * @param {number} date
 * @param {number} departure_time
 * @param {number} time_window
 * @param {number} max_walk_time
 * @param {number} max_bike_time
 * @param {number} max_car_time
 * @param {number} max_trip_duration
 * @param {number} walk_speed
 * @param {number} bike_speed
 * @param {number} max_rides
 * @param {number} max_lts
 * @param {boolean} shortest_path
 * @param {any} scenario
 * @param {boolean} osm_link_ids
 * @param {number} suboptimal_minutes
 * @param {any} fare
 * @param {number} max_fare
 * @returns {Array<any>}
 */
export function itineraries(network, olat, olon, dlat, dlon, mode, transit_modes, egress_mode, date, departure_time, time_window, max_walk_time, max_bike_time, max_car_time, max_trip_duration, walk_speed, bike_speed, max_rides, max_lts, shortest_path, scenario, osm_link_ids, suboptimal_minutes, fare, max_fare) {
    _assertClass(network, Network);
    const val8 = +date;
    const val9 = +departure_time;
    const val10 = +time_window;
    const val11 = +max_walk_time;
    const val12 = +max_bike_time;
    const val13 = +max_car_time;
    const val14 = +max_trip_duration;
    const val15 = +walk_speed;
    const val16 = +bike_speed;
    const val17 = +max_rides;
    const val18 = +max_lts;
    const val19 = +shortest_path;
    const val20 = +osm_link_ids;
    const val21 = +suboptimal_minutes;
    const val22 = +max_fare;
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6;
    try {
        ptr0 = passArrayF64ToWasm0(olat, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(olon, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passArrayF64ToWasm0(dlat, wasm.__wbindgen_malloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passArrayF64ToWasm0(dlon, wasm.__wbindgen_malloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passStringToWasm0(mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len4 = WASM_VECTOR_LEN;
        ptr5 = passStringToWasm0(transit_modes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len5 = WASM_VECTOR_LEN;
        ptr6 = passStringToWasm0(egress_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len6 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr6) { wasm.__wbindgen_free(ptr6, len6 * 1, 1); }
        if (ptr5) { wasm.__wbindgen_free(ptr5, len5 * 1, 1); }
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 1, 1); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 8, 8); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 8, 8); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 8, 8); }
        throw e;
    }
    const ret = wasm.itineraries(network.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, val8, val9, val10, val11, val12, val13, val14, val15, val16, val17, val18, val19, scenario, val20, val21, fare, val22);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * The warnings of a call of `n_pairs` pairs, given the pairs (0-based,
 * ascending, over the whole call) whose itineraries in R5 depend on what
 * ran before (`ItineraryNotes::warnings` of the core).
 * @param {Int32Array | number[]} history_dependent
 * @param {number} n_pairs
 * @returns {any}
 */
export function itinerary_warnings(history_dependent, n_pairs) {
    const val2 = +n_pairs;
    const ptr0 = passArray32ToWasm0(history_dependent, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.itinerary_warnings(ptr0, len0, val2);
    return ret;
}

/**
 * The solution of a location model: `[sites, marginal, cumulative,
 * potential, assigned_site, assigned_minutes, unservable, numbers]`, the
 * indices 0-based (`assigned_site` missing for a demand point that
 * reaches no facility) and `numbers` holding `objective_before`,
 * `objective_greedy`, `objective`, `covered_before`, `covered`,
 * `total_demand`, `unservable_demand` and `n_swaps`. The first
 * `n_existing` sites are the existing facilities; `unreachable_time` not
 * positive for the default.
 * @param {number} n_demand
 * @param {number} n_sites
 * @param {Int32Array | number[]} pair_demand
 * @param {Int32Array | number[]} pair_site
 * @param {Float64Array | number[]} pair_minutes
 * @param {Float64Array | number[]} demand
 * @param {number} n_existing
 * @param {string} model
 * @param {number} n_facilities
 * @param {number} cutoff
 * @param {boolean} swap
 * @param {number} min_gain
 * @param {number} unreachable_time
 * @returns {Array<any>}
 */
export function locate_facilities(n_demand, n_sites, pair_demand, pair_site, pair_minutes, demand, n_existing, model, n_facilities, cutoff, swap, min_gain, unreachable_time) {
    const val6 = +n_demand;
    const val7 = +n_sites;
    const val8 = +n_existing;
    const val9 = +n_facilities;
    const val10 = +cutoff;
    const val11 = +swap;
    const val12 = +min_gain;
    const val13 = +unreachable_time;
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4;
    try {
        ptr0 = passArray32ToWasm0(pair_demand, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArray32ToWasm0(pair_site, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passArrayF64ToWasm0(pair_minutes, wasm.__wbindgen_malloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passArrayF64ToWasm0(demand, wasm.__wbindgen_malloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passStringToWasm0(model, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len4 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 1, 1); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 8, 8); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 8, 8); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 4, 4); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 4, 4); }
        throw e;
    }
    const ret = wasm.locate_facilities(val6, val7, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, val8, ptr4, len4, val9, val10, val11, val12, val13);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * The missing value of the `i32` columns.
 * @returns {number}
 */
export function na_i32() {
    const ret = wasm.na_i32();
    return ret;
}

/**
 * `paretoFrontier()` as `[from, to, percentile, travel_time]`
 * (Int32Array, 0-based indices) and `monetary_cost` (Float64Array);
 * `transit_modes` empty routes by `mode` alone.
 * @param {Network} network
 * @param {Float64Array | number[]} olat
 * @param {Float64Array | number[]} olon
 * @param {Float64Array | number[]} dlat
 * @param {Float64Array | number[]} dlon
 * @param {string} mode
 * @param {string} transit_modes
 * @param {string} egress_mode
 * @param {number} date
 * @param {number} departure_time
 * @param {number} time_window
 * @param {Int32Array | number[]} percentiles
 * @param {number} max_walk_time
 * @param {number} max_bike_time
 * @param {number} max_car_time
 * @param {number} max_trip_duration
 * @param {number} walk_speed
 * @param {number} bike_speed
 * @param {number} max_lts
 * @param {number} max_rides
 * @param {number | null | undefined} seed
 * @param {any} fare
 * @param {Float64Array | number[]} fare_cutoffs
 * @returns {Array<any>}
 */
export function pareto(network, olat, olon, dlat, dlon, mode, transit_modes, egress_mode, date, departure_time, time_window, percentiles, max_walk_time, max_bike_time, max_car_time, max_trip_duration, walk_speed, bike_speed, max_lts, max_rides, seed, fare, fare_cutoffs) {
    _assertClass(network, Network);
    const val10 = +date;
    const val11 = +departure_time;
    const val12 = +time_window;
    const val13 = +max_walk_time;
    const val14 = +max_bike_time;
    const val15 = +max_car_time;
    const val16 = +max_trip_duration;
    const val17 = +walk_speed;
    const val18 = +bike_speed;
    const val19 = +max_lts;
    const val20 = +max_rides;
    const val21 = +(isLikeNone(seed) ? Number.MAX_SAFE_INTEGER : (seed) >> 0);
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8;
    try {
        ptr0 = passArrayF64ToWasm0(olat, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(olon, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passArrayF64ToWasm0(dlat, wasm.__wbindgen_malloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passArrayF64ToWasm0(dlon, wasm.__wbindgen_malloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passStringToWasm0(mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len4 = WASM_VECTOR_LEN;
        ptr5 = passStringToWasm0(transit_modes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len5 = WASM_VECTOR_LEN;
        ptr6 = passStringToWasm0(egress_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len6 = WASM_VECTOR_LEN;
        ptr7 = passArray32ToWasm0(percentiles, wasm.__wbindgen_malloc);
        len7 = WASM_VECTOR_LEN;
        ptr8 = passArrayF64ToWasm0(fare_cutoffs, wasm.__wbindgen_malloc);
        len8 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr8) { wasm.__wbindgen_free(ptr8, len8 * 8, 8); }
        if (ptr7) { wasm.__wbindgen_free(ptr7, len7 * 4, 4); }
        if (ptr6) { wasm.__wbindgen_free(ptr6, len6 * 1, 1); }
        if (ptr5) { wasm.__wbindgen_free(ptr5, len5 * 1, 1); }
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 1, 1); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 8, 8); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 8, 8); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 8, 8); }
        throw e;
    }
    const ret = wasm.pareto(network.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, val10, val11, val12, ptr7, len7, val13, val14, val15, val16, val17, val18, val19, val20, val21, fare, ptr8, len8);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * `[xmin, ymin, xmax, ymax]` of the street network (R5's envelope).
 * @param {Network} network
 * @returns {Float64Array}
 */
export function street_bbox(network) {
    _assertClass(network, Network);
    const ret = wasm.street_bbox(network.__wbg_ptr);
    var v1 = getArrayF64FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 8, 8);
    return v1;
}

/**
 * The street edges (the `edges` of `streetNetworkToSf()`) as an array of
 * columns: `edge_index`, `from_vertex`, `to_vertex` (Int32Array),
 * `osm_id` (Float64Array), `street_class` (Int32Array codes: 0 MOTORWAY,
 * 1 PRIMARY, 2 SECONDARY, 3 TERTIARY, 4 OTHER), `length` (Float64Array),
 * `walk`, `car` (Int32Array 0/1), `car_speed` (Float64Array), `bicycle`
 * (0/1), `bicycle_lts` (Int32Array), the centroid routed to as `lat`,
 * `lon` (Float64Array, NaN without one), and the line of each edge as
 * `n_points` (Int32Array) and flat `x`, `y` (Float64Array).
 * @param {Network} network
 * @returns {Array<any>}
 */
export function street_edges(network) {
    _assertClass(network, Network);
    const ret = wasm.street_edges(network.__wbg_ptr);
    return ret;
}

/**
 * Street-only travel-time matrix: the reported pairs as
 * `[from, to, minutes]` Int32Arrays, origin-major.
 * @param {Network} network
 * @param {Float64Array | number[]} olat
 * @param {Float64Array | number[]} olon
 * @param {Float64Array | number[]} dlat
 * @param {Float64Array | number[]} dlon
 * @param {string} mode
 * @param {number} walk_speed
 * @param {number} bike_speed
 * @param {number} max_lts
 * @param {number} max_trip_duration
 * @param {any} scenario
 * @returns {Array<any>}
 */
export function street_ttm(network, olat, olon, dlat, dlon, mode, walk_speed, bike_speed, max_lts, max_trip_duration, scenario) {
    _assertClass(network, Network);
    const val6 = +walk_speed;
    const val7 = +bike_speed;
    const val8 = +max_lts;
    const val9 = +max_trip_duration;
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4;
    try {
        ptr0 = passArrayF64ToWasm0(olat, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(olon, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passArrayF64ToWasm0(dlat, wasm.__wbindgen_malloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passArrayF64ToWasm0(dlon, wasm.__wbindgen_malloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passStringToWasm0(mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len4 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 1, 1); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 8, 8); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 8, 8); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 8, 8); }
        throw e;
    }
    const ret = wasm.street_ttm(network.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, val6, val7, val8, val9, scenario);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * The vertices of `streetNetworkToSf()`: `[index (Int32Array),
 * lat, lon (Float64Array), park_and_ride, bike_sharing (Uint8Array)]`.
 * @param {Network} network
 * @returns {Array<any>}
 */
export function street_vertices(network) {
    _assertClass(network, Network);
    const ret = wasm.street_vertices(network.__wbg_ptr);
    return ret;
}

/**
 * The routes of `transitNetworkToSf()`: `[agency_id,
 * agency_name, route_id, long_name, short_name, mode]` (arrays, null where
 * blank) and the line of each row as `n_points` (Int32Array) and flat `x`,
 * `y` (Float64Array).
 * @param {Network} network
 * @returns {Array<any>}
 */
export function transit_routes(network) {
    _assertClass(network, Network);
    const ret = wasm.transit_routes(network.__wbg_ptr);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * The operating dates of the network's services (`yyyymmdd`, ascending),
 * as `checkTransitAvailability()` discovers them.
 * @param {Network} network
 * @returns {Uint32Array}
 */
export function transit_service_dates(network) {
    _assertClass(network, Network);
    const ret = wasm.transit_service_dates(network.__wbg_ptr);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    return v1;
}

/**
 * The stops of `transitNetworkToSf()`: `[stop_index
 * (Int32Array), stop_id, stop_name (arrays, null where blank), lat, lon
 * (Float64Array, NaN for stops not linked to the streets),
 * linked_to_street (Uint8Array)]`.
 * @param {Network} network
 * @returns {Array<any>}
 */
export function transit_stops(network) {
    _assertClass(network, Network);
    const ret = wasm.transit_stops(network.__wbg_ptr);
    return ret;
}

/**
 * Public transport travel-time matrix: the reported pairs as `from`, `to`
 * and one Int32Array per percentile (ascending), with `NA_I32` where
 * there is no time; origin-major.
 * @param {Network} network
 * @param {Float64Array | number[]} olat
 * @param {Float64Array | number[]} olon
 * @param {Float64Array | number[]} dlat
 * @param {Float64Array | number[]} dlon
 * @param {string} transit_modes
 * @param {string} access_mode
 * @param {string} egress_mode
 * @param {number} bike_speed
 * @param {number} max_lts
 * @param {number} max_bike_time
 * @param {number} max_car_time
 * @param {number} date
 * @param {number} departure_time
 * @param {number} time_window
 * @param {Int32Array | number[]} percentiles
 * @param {number} max_walk_time
 * @param {number} max_trip_duration
 * @param {number} walk_speed
 * @param {number} max_rides
 * @param {number} draws_per_minute
 * @param {number | null | undefined} seed
 * @param {any} scenario
 * @param {any} fare
 * @param {number} max_fare
 * @returns {Array<any>}
 */
export function transit_ttm(network, olat, olon, dlat, dlon, transit_modes, access_mode, egress_mode, bike_speed, max_lts, max_bike_time, max_car_time, date, departure_time, time_window, percentiles, max_walk_time, max_trip_duration, walk_speed, max_rides, draws_per_minute, seed, scenario, fare, max_fare) {
    _assertClass(network, Network);
    const val9 = +bike_speed;
    const val10 = +max_lts;
    const val11 = +max_bike_time;
    const val12 = +max_car_time;
    const val13 = +date;
    const val14 = +departure_time;
    const val15 = +time_window;
    const val16 = +max_walk_time;
    const val17 = +max_trip_duration;
    const val18 = +walk_speed;
    const val19 = +max_rides;
    const val20 = +draws_per_minute;
    const val21 = +(isLikeNone(seed) ? Number.MAX_SAFE_INTEGER : (seed) >> 0);
    const val22 = +max_fare;
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, ptr7, len7;
    try {
        ptr0 = passArrayF64ToWasm0(olat, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(olon, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passArrayF64ToWasm0(dlat, wasm.__wbindgen_malloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passArrayF64ToWasm0(dlon, wasm.__wbindgen_malloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passStringToWasm0(transit_modes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len4 = WASM_VECTOR_LEN;
        ptr5 = passStringToWasm0(access_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len5 = WASM_VECTOR_LEN;
        ptr6 = passStringToWasm0(egress_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len6 = WASM_VECTOR_LEN;
        ptr7 = passArray32ToWasm0(percentiles, wasm.__wbindgen_malloc);
        len7 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr7) { wasm.__wbindgen_free(ptr7, len7 * 4, 4); }
        if (ptr6) { wasm.__wbindgen_free(ptr6, len6 * 1, 1); }
        if (ptr5) { wasm.__wbindgen_free(ptr5, len5 * 1, 1); }
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 1, 1); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 8, 8); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 8, 8); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 8, 8); }
        throw e;
    }
    const ret = wasm.transit_ttm(network.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, val9, val10, val11, val12, val13, val14, val15, ptr7, len7, val16, val17, val18, val19, val20, val21, scenario, fare, val22);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Options between `o[i]` and `d[i]` as an array of three arrays of
 * columns. Options: `pair` (0-based), `option`, `transfers`,
 * `n_departures` (Int32Array), `total_time`, `fare` (NaN without a fare
 * structure), `walk_time` (Float64Array), `departure_time`,
 * `arrival_time`, `first_departure`, `last_departure`, `description`
 * (strings). Departures: `pair`, `option` (Int32Array), `total_time`
 * (Float64Array), `departure_time`, `arrival_time` (strings). Legs (empty
 * without `itineraries`): `pair`, `option`, `segment`, `distance`,
 * `n_points` (Int32Array), `cumulative_fare`, `segment_duration`, `wait`,
 * `x`, `y` (Float64Array), `mode`, `route` and, with `osm_link_ids`,
 * `osm_id_list`, `edge_id_list`, `board_stop_id`, `alight_stop_id`
 * (strings).
 * @param {Network} network
 * @param {Float64Array | number[]} olat
 * @param {Float64Array | number[]} olon
 * @param {Float64Array | number[]} dlat
 * @param {Float64Array | number[]} dlon
 * @param {string} mode
 * @param {string} transit_modes
 * @param {string} egress_mode
 * @param {number} date
 * @param {number} departure_time
 * @param {number} time_window
 * @param {string[]} criteria
 * @param {number} max_walk_time
 * @param {number} max_bike_time
 * @param {number} max_car_time
 * @param {number} max_trip_duration
 * @param {number} walk_speed
 * @param {number} bike_speed
 * @param {number} max_rides
 * @param {number} max_lts
 * @param {any} scenario
 * @param {any} fare
 * @param {number} max_fare
 * @param {number} max_options
 * @param {boolean} itineraries
 * @param {boolean} osm_link_ids
 * @param {number} walk_resolution
 * @returns {Array<any>}
 */
export function trip_options_js(network, olat, olon, dlat, dlon, mode, transit_modes, egress_mode, date, departure_time, time_window, criteria, max_walk_time, max_bike_time, max_car_time, max_trip_duration, walk_speed, bike_speed, max_rides, max_lts, scenario, fare, max_fare, max_options, itineraries, osm_link_ids, walk_resolution) {
    _assertClass(network, Network);
    const val9 = +date;
    const val10 = +departure_time;
    const val11 = +time_window;
    const val12 = +max_walk_time;
    const val13 = +max_bike_time;
    const val14 = +max_car_time;
    const val15 = +max_trip_duration;
    const val16 = +walk_speed;
    const val17 = +bike_speed;
    const val18 = +max_rides;
    const val19 = +max_lts;
    const val20 = +max_fare;
    const val21 = +max_options;
    const val22 = +itineraries;
    const val23 = +osm_link_ids;
    const val24 = +walk_resolution;
    let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, ptr7, len7;
    try {
        ptr0 = passArrayF64ToWasm0(olat, wasm.__wbindgen_malloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passArrayF64ToWasm0(olon, wasm.__wbindgen_malloc);
        len1 = WASM_VECTOR_LEN;
        ptr2 = passArrayF64ToWasm0(dlat, wasm.__wbindgen_malloc);
        len2 = WASM_VECTOR_LEN;
        ptr3 = passArrayF64ToWasm0(dlon, wasm.__wbindgen_malloc);
        len3 = WASM_VECTOR_LEN;
        ptr4 = passStringToWasm0(mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len4 = WASM_VECTOR_LEN;
        ptr5 = passStringToWasm0(transit_modes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len5 = WASM_VECTOR_LEN;
        ptr6 = passStringToWasm0(egress_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len6 = WASM_VECTOR_LEN;
        ptr7 = passArrayJsValueToWasm0(criteria, wasm.__wbindgen_malloc);
        len7 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr7) { wasm.__externref_drop_slice(ptr7, len7); wasm.__wbindgen_free(ptr7, len7 * 4, 4); }
        if (ptr6) { wasm.__wbindgen_free(ptr6, len6 * 1, 1); }
        if (ptr5) { wasm.__wbindgen_free(ptr5, len5 * 1, 1); }
        if (ptr4) { wasm.__wbindgen_free(ptr4, len4 * 1, 1); }
        if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 8, 8); }
        if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 8, 8); }
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 8, 8); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 8, 8); }
        throw e;
    }
    const ret = wasm.trip_options_js(network.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, val9, val10, val11, ptr7, len7, val12, val13, val14, val15, val16, val17, val18, val19, scenario, fare, val20, val21, val22, val23, val24);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_208446807d0d1e30: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_boolean_get_4f1baf9a00be3c74: function(arg0) {
            const v = arg0;
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_is_null_c22311b1cc228e3a: function(arg0) {
            const ret = arg0 === null;
            return ret;
        },
        __wbg___wbindgen_is_undefined_be5b34249ff1fc15: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_number_get_934b640d0889af78: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64((arg0 >>> 0) + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_7ce8eee7fd327545: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_4c88d8bf264fd984: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_from_eac91875c8a5ffa4: function(arg0) {
            const ret = Array.from(arg0);
            return ret;
        },
        __wbg_get_2080c49015e35254: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_get_d7176cfb76d71114: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_get_unchecked_e6325ad374eff861: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_instanceof_Float64Array_4acace5b49aaf791: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Float64Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Int32Array_6d09616361bcf0c6: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Int32Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Uint8Array_d7aeb0dcb3108fea: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_isArray_bcf1376d0d732d08: function(arg0) {
            const ret = Array.isArray(arg0);
            return ret;
        },
        __wbg_length_03a2b32a9178fe11: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_186163212ffe79c3: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_5a8bb8abe1e54e15: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_b03a0fabb109b1e9: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_new_37eb45aaf5eeb820: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_new_cd4348290afae586: function() {
            const ret = new Object();
            return ret;
        },
        __wbg_new_from_slice_aaf85a9746711968: function(arg0, arg1) {
            const ret = new Float64Array(getArrayF64FromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_from_slice_b8daf545a41c6aa0: function(arg0, arg1) {
            const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_from_slice_e0c60a796e12bb7c: function(arg0, arg1) {
            const ret = new Int32Array(getArrayI32FromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_prototypesetcall_13162965d1fe7aa3: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_prototypesetcall_153eecd0d28a42a5: function(arg0, arg1, arg2) {
            Int32Array.prototype.set.call(getArrayI32FromWasm0(arg0, arg1), arg2);
        },
        __wbg_prototypesetcall_a366c038a3918afa: function(arg0, arg1, arg2) {
            Float64Array.prototype.set.call(getArrayF64FromWasm0(arg0, arg1), arg2);
        },
        __wbg_push_c9f4893fc209f768: function(arg0, arg1) {
            const ret = arg0.push(arg1);
            return ret;
        },
        __wbg_set_2793479888329cb6: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = Reflect.set(arg0, arg1, arg2);
            return ret;
        }, arguments); },
        __wbindgen_generic_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_generic_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            table.set(0, undefined);
            table.set(1024, undefined);
            table.set(1025, null);
            table.set(1026, true);
            table.set(1027, false);
        },
    };
    return {
        __proto__: null,
        "./peabiru_bg.js": import0,
    };
}

const NetworkFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_network_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

function getArrayF64FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat64ArrayMemory0().subarray(ptr / 8, ptr / 8 + len);
}

function getArrayI32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getInt32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayU32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat64ArrayMemory0 = null;
function getFloat64ArrayMemory0() {
    if (cachedFloat64ArrayMemory0 === null || cachedFloat64ArrayMemory0.byteLength === 0) {
        cachedFloat64ArrayMemory0 = new Float64Array(wasm.memory.buffer);
    }
    return cachedFloat64ArrayMemory0;
}

let cachedInt32ArrayMemory0 = null;
function getInt32ArrayMemory0() {
    if (cachedInt32ArrayMemory0 === null || cachedInt32ArrayMemory0.byteLength === 0) {
        cachedInt32ArrayMemory0 = new Int32Array(wasm.memory.buffer);
    }
    return cachedInt32ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

let cachedUint32ArrayMemory0 = null;
function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getUint32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayF64ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 8, 8) >>> 0;
    getFloat64ArrayMemory0().set(arg, ptr / 8);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
        const add = addToExternrefTable0(array[i]);
        getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(bytes) {
    numBytesDecoded += bytes.length;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
        cachedTextDecoder.decode();
        numBytesDecoded = bytes.length;
    }
    return cachedTextDecoder.decode(bytes);
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat64ArrayMemory0 = null;
    cachedInt32ArrayMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

let __wbg_init_promise;
async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;
    if (__wbg_init_promise === undefined) {
        __wbg_init_promise = __wbg_init_once(module_or_path);
        __wbg_init_promise.catch(() => { __wbg_init_promise = undefined; });
    }
    return __wbg_init_promise;
}

async function __wbg_init_once(module_or_path) {

    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('peabiru_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
