export namespace main {
	
	export class TrackDTO {
	    title: string;
	    artist: string;
	    url: string;
	    durationSec: number;
	
	    static createFrom(source: any = {}) {
	        return new TrackDTO(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.title = source["title"];
	        this.artist = source["artist"];
	        this.url = source["url"];
	        this.durationSec = source["durationSec"];
	    }
	}
	export class ResolveResult {
	    tracks?: TrackDTO[];
	    needsAuth?: boolean;
	    error?: string;
	
	    static createFrom(source: any = {}) {
	        return new ResolveResult(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.tracks = this.convertValues(source["tracks"], TrackDTO);
	        this.needsAuth = source["needsAuth"];
	        this.error = source["error"];
	    }
	
		convertValues(a: any, classs: any, asMap: boolean = false): any {
		    if (!a) {
		        return a;
		    }
		    if (a.slice && a.map) {
		        return (a as any[]).map(elem => this.convertValues(elem, classs));
		    } else if ("object" === typeof a) {
		        if (asMap) {
		            for (const key of Object.keys(a)) {
		                a[key] = new classs(a[key]);
		            }
		            return a;
		        }
		        return new classs(a);
		    }
		    return a;
		}
	}
	export class SpotifyCredsDTO {
	    clientId: string;
	    clientSecret: string;
	
	    static createFrom(source: any = {}) {
	        return new SpotifyCredsDTO(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.clientId = source["clientId"];
	        this.clientSecret = source["clientSecret"];
	    }
	}

}

