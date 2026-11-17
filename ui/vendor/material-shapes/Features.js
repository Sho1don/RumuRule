/*
 * Copyright 2023 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
import { DistanceEpsilon } from './Utils.js';
export class Feature {
    cubics;
    constructor(cubics) {
        this.cubics = cubics;
    }
    static buildIgnorableFeature(cubics) {
        return Feature.validated(new Edge(cubics));
    }
    static buildEdge(cubic) {
        return new Edge([cubic]);
    }
    static buildConvexCorner(cubics) {
        return Feature.validated(new Corner(cubics, true));
    }
    static buildConcaveCorner(cubics) {
        return Feature.validated(new Corner(cubics, false));
    }
    static validated(feature) {
        if (feature.cubics.length === 0) {
            throw new Error("Features need at least one cubic.");
        }
        if (!Feature.isContinuous(feature)) {
            throw new Error("Feature must be continuous, with the anchor points of all cubics " +
                "matching the anchor points of the preceding and succeeding cubics");
        }
        return feature;
    }
    static isContinuous(feature) {
        let prevCubic = feature.cubics[0];
        for (let i = 1; i < feature.cubics.length; i++) {
            const cubic = feature.cubics[i];
            if (Math.abs(cubic.anchor0X - prevCubic.anchor1X) > DistanceEpsilon ||
                Math.abs(cubic.anchor0Y - prevCubic.anchor1Y) > DistanceEpsilon) {
                return false;
            }
            prevCubic = cubic;
        }
        return true;
    }
}
export class Edge extends Feature {
    constructor(cubics) {
        super(cubics);
    }
    transformed(f) {
        return new Edge(this.cubics.map(c => c.transformed(f)));
    }
    reversed() {
        return new Edge(this.cubics.map(c => c.reverse()).reverse());
    }
    equals(other) {
        if (this === other)
            return true;
        if (!(other instanceof Edge))
            return false;
        if (this.cubics.length !== other.cubics.length)
            return false;
        return this.cubics.every((c, i) => c.equals(other.cubics[i]));
    }
    get isIgnorableFeature() { return true; }
    get isEdge() { return true; }
    get isConvexCorner() { return false; }
    get isConcaveCorner() { return false; }
    toString() { return "Edge"; }
}
export class Corner extends Feature {
    convex;
    constructor(cubics, convex = true) {
        super(cubics);
        this.convex = convex;
    }
    transformed(f) {
        return new Corner(this.cubics.map(c => c.transformed(f)), this.convex);
    }
    reversed() {
        return new Corner(this.cubics.map(c => c.reverse()).reverse(), !this.convex);
    }
    equals(other) {
        if (this === other)
            return true;
        if (!(other instanceof Corner))
            return false;
        if (this.convex !== other.convex)
            return false;
        if (this.cubics.length !== other.cubics.length)
            return false;
        return this.cubics.every((c, i) => c.equals(other.cubics[i]));
    }
    get isIgnorableFeature() { return false; }
    get isEdge() { return false; }
    get isConvexCorner() { return this.convex; }
    get isConcaveCorner() { return !this.convex; }
    toString() {
        return `Corner: cubics=${this.cubics.map(c => `[${c}]`).join(", ")} convex=${this.convex}`;
    }
}
//# sourceMappingURL=Features.js.map