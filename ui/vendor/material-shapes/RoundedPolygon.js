/*
 * Copyright 2022 The Android Open Source Project
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
import { CornerRounding } from './CornerRounding.js';
import { Cubic } from './Cubic.js';
import { Point } from './Point.js';
import { convex, debugLog, directionVector, distance, DistanceEpsilon, FloatPi, radialToCartesian, square } from './Utils.js';
import { Corner, Edge } from './Features.js';
export class RoundedPolygon {
    features;
    center;
    cubics;
    constructor(features, center) {
        this.features = features;
        this.center = center;
        this.cubics = this.calculateCubics();
        this.validate();
    }
    get centerX() {
        return this.center.x;
    }
    get centerY() {
        return this.center.y;
    }
    calculateCubics() {
        const cubics = [];
        let firstCubic = null;
        let lastCubic = null;
        let firstFeatureSplitStart = null;
        let firstFeatureSplitEnd = null;
        if (this.features.length > 0 && this.features[0].cubics.length === 3) {
            const centerCubic = this.features[0].cubics[1];
            const [start, end] = centerCubic.split(0.5);
            firstFeatureSplitStart = [this.features[0].cubics[0], start];
            firstFeatureSplitEnd = [end, this.features[0].cubics[2]];
        }
        for (let i = 0; i <= this.features.length; i++) {
            let featureCubics;
            if (i === 0 && firstFeatureSplitEnd != null) {
                featureCubics = firstFeatureSplitEnd;
            }
            else if (i === this.features.length) {
                if (firstFeatureSplitStart != null) {
                    featureCubics = firstFeatureSplitStart;
                }
                else {
                    break;
                }
            }
            else {
                featureCubics = this.features[i].cubics;
            }
            for (const cubic of featureCubics) {
                if (!cubic.zeroLength()) {
                    if (lastCubic != null) {
                        cubics.push(lastCubic);
                    }
                    lastCubic = cubic;
                    if (firstCubic == null) {
                        firstCubic = cubic;
                    }
                }
                else {
                    if (lastCubic != null) {
                        const newPoints = [...lastCubic.points];
                        newPoints[6] = cubic.anchor1X;
                        newPoints[7] = cubic.anchor1Y;
                        lastCubic = new Cubic(newPoints);
                    }
                }
            }
        }
        if (lastCubic != null && firstCubic != null) {
            cubics.push(new Cubic([
                lastCubic.anchor0X, lastCubic.anchor0Y,
                lastCubic.control0X, lastCubic.control0Y,
                lastCubic.control1X, lastCubic.control1Y,
                firstCubic.anchor0X, firstCubic.anchor0Y,
            ]));
        }
        else {
            cubics.push(new Cubic([
                this.centerX, this.centerY, this.centerX, this.centerY,
                this.centerX, this.centerY, this.centerX, this.centerY
            ]));
        }
        return cubics;
    }
    validate() {
        if (this.cubics.length === 0)
            return;
        let prevCubic = this.cubics[this.cubics.length - 1];
        debugLog("RoundedPolygon", () => `Cubic-1 = ${prevCubic}`);
        for (let index = 0; index < this.cubics.length; index++) {
            const cubic = this.cubics[index];
            debugLog("RoundedPolygon", () => `Cubic = ${cubic}`);
            if (Math.abs(cubic.anchor0X - prevCubic.anchor1X) > DistanceEpsilon ||
                Math.abs(cubic.anchor0Y - prevCubic.anchor1Y) > DistanceEpsilon) {
                debugLog("RoundedPolygon", () => `Ix: ${index} | (${cubic.anchor0X},${cubic.anchor0Y}) vs ${prevCubic}`);
                throw new Error("RoundedPolygon must be contiguous, with the anchor points of all curves " +
                    "matching the anchor points of the preceding and succeeding cubics");
            }
            prevCubic = cubic;
        }
    }
    transformed(f) {
        const center = this.center.transformed(f);
        const newFeatures = this.features.map(feature => feature.transformed(f));
        return new RoundedPolygon(newFeatures, center);
    }
    normalized() {
        const bounds = this.calculateBounds();
        const width = bounds[2] - bounds[0];
        const height = bounds[3] - bounds[1];
        const side = Math.max(width, height);
        const offsetX = (side - width) / 2 - bounds[0];
        const offsetY = (side - height) / 2 - bounds[1];
        return this.transformed({
            transform(x, y) {
                return { first: (x + offsetX) / side, second: (y + offsetY) / side };
            }
        });
    }
    toString() {
        return "[RoundedPolygon." +
            " Cubics = " + this.cubics.join(", ") +
            " || Features = " + this.features.join(", ") +
            " || Center = (" + this.centerX + ", " + this.centerY + ")]";
    }
    calculateMaxBounds(bounds = [0, 0, 0, 0]) {
        if (bounds.length < 4)
            throw new Error("Required bounds size of 4");
        let maxDistSquared = 0;
        for (const cubic of this.cubics) {
            // Accumulate SQUARED distances; sqrt once at the end (Kotlin parity).
            const adx = cubic.anchor0X - this.centerX, ady = cubic.anchor0Y - this.centerY;
            const anchorDistSquared = adx * adx + ady * ady;
            const middlePoint = cubic.pointOnCurve(0.5);
            const mdx = middlePoint.x - this.centerX, mdy = middlePoint.y - this.centerY;
            const middleDistSquared = mdx * mdx + mdy * mdy;
            maxDistSquared = Math.max(maxDistSquared, anchorDistSquared, middleDistSquared);
        }
        const dist = Math.sqrt(maxDistSquared);
        bounds[0] = this.centerX - dist;
        bounds[1] = this.centerY - dist;
        bounds[2] = this.centerX + dist;
        bounds[3] = this.centerY + dist;
        return bounds;
    }
    calculateBounds(bounds = [0, 0, 0, 0], approximate = true) {
        if (bounds.length < 4)
            throw new Error("Required bounds size of 4");
        let minX = Infinity;
        let minY = Infinity;
        let maxX = -Infinity;
        let maxY = -Infinity;
        const tempBounds = [0, 0, 0, 0];
        for (const cubic of this.cubics) {
            cubic.calculateBounds(tempBounds, approximate);
            minX = Math.min(minX, tempBounds[0]);
            minY = Math.min(minY, tempBounds[1]);
            maxX = Math.max(maxX, tempBounds[2]);
            maxY = Math.max(maxY, tempBounds[3]);
        }
        bounds[0] = minX;
        bounds[1] = minY;
        bounds[2] = maxX;
        bounds[3] = maxY;
        return bounds;
    }
    equals(other) {
        if (this === other)
            return true;
        if (!(other instanceof RoundedPolygon))
            return false;
        if (this.features.length !== other.features.length)
            return false;
        for (let i = 0; i < this.features.length; i++) {
            if (!this.features[i].equals(other.features[i])) {
                return false;
            }
        }
        return true;
    }
    static create(arg1, arg2, arg3, arg4, arg5, arg6) {
        if (typeof arg1 === 'number') {
            // Constructor with numVertices
            const numVertices = arg1;
            const radius = arg2 ?? 1;
            const centerX = arg3 ?? 0;
            const centerY = arg4 ?? 0;
            const rounding = arg5 ?? CornerRounding.Unrounded;
            const perVertexRounding = arg6 ?? undefined;
            return RoundedPolygon.createFromNumVertices(numVertices, radius, centerX, centerY, rounding, perVertexRounding);
        }
        else if (Array.isArray(arg1) && (arg1.length === 0 || typeof arg1[0] === 'number')) {
            // Constructor with vertices
            const vertices = arg1;
            const rounding = arg2 ?? CornerRounding.Unrounded;
            const perVertexRounding = arg3 ?? undefined;
            const centerX = arg4 ?? -Infinity;
            const centerY = arg5 ?? -Infinity;
            return RoundedPolygon.createFromVertices(vertices, rounding, perVertexRounding, centerX, centerY);
        }
        else if (Array.isArray(arg1)) {
            // Constructor with features
            const features = arg1;
            const centerX = arg2 ?? NaN;
            const centerY = arg3 ?? NaN;
            return RoundedPolygon.createFromFeatures(features, centerX, centerY);
        }
        else {
            // Copy constructor
            const source = arg1;
            return new RoundedPolygon(source.features, source.center);
        }
    }
    static createFromNumVertices(numVertices, radius = 1, centerX = 0, centerY = 0, rounding = CornerRounding.Unrounded, perVertexRounding) {
        if (numVertices < 3)
            throw new Error("Polygons must have at least 3 vertices");
        const vertices = verticesFromNumVerts(numVertices, radius, centerX, centerY);
        return RoundedPolygon.createFromVertices(vertices, rounding, perVertexRounding, centerX, centerY);
    }
    static createFromVertices(vertices, rounding = CornerRounding.Unrounded, perVertexRounding, centerX = -Infinity, centerY = -Infinity) {
        if (vertices.length < 6)
            throw new Error("Polygons must have at least 3 vertices");
        if (vertices.length % 2 === 1)
            throw new Error("The vertices array should have even size");
        const n = vertices.length / 2;
        if (perVertexRounding && perVertexRounding.length !== n) {
            throw new Error("perVertexRounding list should be either null or " +
                "the same size as the number of vertices (vertices.size / 2)");
        }
        const roundedCorners = [];
        for (let i = 0; i < n; i++) {
            const vtxRounding = perVertexRounding?.[i] ?? rounding;
            const prevIndex = ((i + n - 1) % n) * 2;
            const nextIndex = ((i + 1) % n) * 2;
            roundedCorners.push(new RoundedCorner(new Point(vertices[prevIndex], vertices[prevIndex + 1]), new Point(vertices[i * 2], vertices[i * 2 + 1]), new Point(vertices[nextIndex], vertices[nextIndex + 1]), vtxRounding));
        }
        const cutAdjusts = Array.from({ length: n }, (_, ix) => {
            const expectedRoundCut = roundedCorners[ix].expectedRoundCut +
                roundedCorners[(ix + 1) % n].expectedRoundCut;
            const expectedCut = roundedCorners[ix].expectedCut +
                roundedCorners[(ix + 1) % n].expectedCut;
            const vtxX = vertices[ix * 2];
            const vtxY = vertices[ix * 2 + 1];
            const nextVtxX = vertices[((ix + 1) % n) * 2];
            const nextVtxY = vertices[((ix + 1) % n) * 2 + 1];
            const sideSize = distance(vtxX - nextVtxX, vtxY - nextVtxY);
            if (expectedRoundCut > sideSize) {
                return { roundCutRatio: sideSize / expectedRoundCut, cutRatio: 0 };
            }
            else if (expectedCut > sideSize) {
                return {
                    roundCutRatio: 1,
                    cutRatio: (sideSize - expectedRoundCut) / (expectedCut - expectedRoundCut)
                };
            }
            else {
                return { roundCutRatio: 1, cutRatio: 1 };
            }
        });
        const corners = [];
        for (let i = 0; i < n; i++) {
            const allowedCuts = [];
            for (let delta = 0; delta <= 1; delta++) {
                const { roundCutRatio, cutRatio } = cutAdjusts[(i + n - 1 + delta) % n];
                allowedCuts.push(roundedCorners[i].expectedRoundCut * roundCutRatio +
                    (roundedCorners[i].expectedCut - roundedCorners[i].expectedRoundCut) * cutRatio);
            }
            corners.push(roundedCorners[i].getCubics(allowedCuts[0], allowedCuts[1]));
        }
        const tempFeatures = [];
        for (let i = 0; i < n; i++) {
            const prevVtxIndex = (i + n - 1) % n;
            const nextVtxIndex = (i + 1) % n;
            const currVertex = new Point(vertices[i * 2], vertices[i * 2 + 1]);
            const prevVertex = new Point(vertices[prevVtxIndex * 2], vertices[prevVtxIndex * 2 + 1]);
            const nextVertex = new Point(vertices[nextVtxIndex * 2], vertices[nextVtxIndex * 2 + 1]);
            const isConvex = convex(prevVertex, currVertex, nextVertex);
            tempFeatures.push(new Corner(corners[i], isConvex));
            const lastCornerCubic = corners[i][corners[i].length - 1];
            const nextCornerCubic = corners[(i + 1) % n][0];
            tempFeatures.push(new Edge([
                Cubic.straightLine(lastCornerCubic.anchor1X, lastCornerCubic.anchor1Y, nextCornerCubic.anchor0X, nextCornerCubic.anchor0Y)
            ]));
        }
        let cx, cy;
        if (centerX === -Infinity || centerY === -Infinity) {
            const center = calculateCenter(vertices);
            cx = center.x;
            cy = center.y;
        }
        else {
            cx = centerX;
            cy = centerY;
        }
        return new RoundedPolygon(tempFeatures, new Point(cx, cy));
    }
    static createFromFeatures(features, centerX = NaN, centerY = NaN) {
        if (features.length < 2)
            throw new Error("Polygons must have at least 2 features");
        const vertices = [];
        for (const feature of features) {
            for (const cubic of feature.cubics) {
                vertices.push(cubic.anchor0X, cubic.anchor0Y);
            }
        }
        const center = calculateCenter(vertices);
        const cX = isNaN(centerX) ? center.x : centerX;
        const cY = isNaN(centerY) ? center.y : centerY;
        return new RoundedPolygon(features, new Point(cX, cY));
    }
}
function calculateCenter(vertices) {
    let cumulativeX = 0;
    let cumulativeY = 0;
    for (let i = 0; i < vertices.length; i += 2) {
        cumulativeX += vertices[i];
        cumulativeY += vertices[i + 1];
    }
    return new Point(cumulativeX / (vertices.length / 2), cumulativeY / (vertices.length / 2));
}
class RoundedCorner {
    p0;
    p1;
    p2;
    rounding;
    d1;
    d2;
    cornerRadius;
    smoothing;
    cosAngle;
    sinAngle;
    expectedRoundCut;
    center = new Point(0, 0);
    constructor(p0, p1, p2, rounding = null) {
        this.p0 = p0;
        this.p1 = p1;
        this.p2 = p2;
        this.rounding = rounding;
        const v01 = p0.minus(p1);
        const v21 = p2.minus(p1);
        const d01 = v01.getDistance();
        const d21 = v21.getDistance();
        if (d01 > 0 && d21 > 0) {
            this.d1 = v01.div(d01);
            this.d2 = v21.div(d21);
            this.cornerRadius = rounding?.radius ?? 0;
            this.smoothing = rounding?.smoothing ?? 0;
            this.cosAngle = this.d1.dotProduct(this.d2);
            this.sinAngle = Math.sqrt(1 - square(this.cosAngle));
            this.expectedRoundCut = this.sinAngle > 1e-3
                ? this.cornerRadius * (this.cosAngle + 1) / this.sinAngle
                : 0;
        }
        else {
            this.d1 = new Point(0, 0);
            this.d2 = new Point(0, 0);
            this.cornerRadius = 0;
            this.smoothing = 0;
            this.cosAngle = 0;
            this.sinAngle = 0;
            this.expectedRoundCut = 0;
        }
    }
    get expectedCut() {
        return (1 + this.smoothing) * this.expectedRoundCut;
    }
    getCubics(allowedCut0, allowedCut1 = allowedCut0) {
        const allowedCut = Math.min(allowedCut0, allowedCut1);
        if (this.expectedRoundCut < DistanceEpsilon ||
            allowedCut < DistanceEpsilon ||
            this.cornerRadius < DistanceEpsilon) {
            this.center = this.p1;
            return [Cubic.straightLine(this.p1.x, this.p1.y, this.p1.x, this.p1.y)];
        }
        const actualRoundCut = Math.min(allowedCut, this.expectedRoundCut);
        const actualSmoothing0 = this.calculateActualSmoothingValue(allowedCut0);
        const actualSmoothing1 = this.calculateActualSmoothingValue(allowedCut1);
        const actualR = this.cornerRadius * actualRoundCut / this.expectedRoundCut;
        const centerDistance = Math.sqrt(square(actualR) + square(actualRoundCut));
        this.center = this.p1.plus(this.d1.plus(this.d2).div(2).getDirection().times(centerDistance));
        const circleIntersection0 = this.p1.plus(this.d1.times(actualRoundCut));
        const circleIntersection2 = this.p1.plus(this.d2.times(actualRoundCut));
        const flanking0 = this.computeFlankingCurve(actualRoundCut, actualSmoothing0, this.p1, this.p0, circleIntersection0, circleIntersection2, this.center, actualR);
        const flanking2 = this.computeFlankingCurve(actualRoundCut, actualSmoothing1, this.p1, this.p2, circleIntersection2, circleIntersection0, this.center, actualR).reverse();
        return [
            flanking0,
            Cubic.circularArc(this.center.x, this.center.y, flanking0.anchor1X, flanking0.anchor1Y, flanking2.anchor0X, flanking2.anchor0Y),
            flanking2
        ];
    }
    calculateActualSmoothingValue(allowedCut) {
        if (allowedCut > this.expectedCut) {
            return this.smoothing;
        }
        else if (allowedCut > this.expectedRoundCut) {
            return this.smoothing * (allowedCut - this.expectedRoundCut) /
                (this.expectedCut - this.expectedRoundCut);
        }
        else {
            return 0;
        }
    }
    computeFlankingCurve(actualRoundCut, actualSmoothingValues, corner, sideStart, circleSegmentIntersection, otherCircleSegmentIntersection, circleCenter, actualR) {
        const sideDirection = sideStart.minus(corner).getDirection();
        const curveStart = corner.plus(sideDirection.times(actualRoundCut * (1 + actualSmoothingValues)));
        const p = Point.interpolate(circleSegmentIntersection, circleSegmentIntersection.plus(otherCircleSegmentIntersection).div(2), actualSmoothingValues);
        const curveEnd = circleCenter.plus(directionVector(p.x - circleCenter.x, p.y - circleCenter.y).times(actualR));
        const circleTangent = curveEnd.minus(circleCenter).rotate90();
        const anchorEnd = lineIntersection(sideStart, sideDirection, curveEnd, circleTangent) ??
            circleSegmentIntersection;
        const anchorStart = curveStart.plus(anchorEnd.times(2)).div(3);
        return Cubic.fromPoints(curveStart, anchorStart, anchorEnd, curveEnd);
    }
}
function lineIntersection(p0, d0, p1, d1) {
    const rotatedD1 = d1.rotate90();
    const den = d0.dotProduct(rotatedD1);
    if (Math.abs(den) < DistanceEpsilon)
        return null;
    const num = p1.minus(p0).dotProduct(rotatedD1);
    if (Math.abs(den) < DistanceEpsilon * Math.abs(num))
        return null;
    const k = num / den;
    return p0.plus(d0.times(k));
}
function verticesFromNumVerts(numVertices, radius, centerX, centerY) {
    const result = [];
    for (let i = 0; i < numVertices; i++) {
        const vertex = radialToCartesian(radius, (FloatPi / numVertices * 2 * i))
            .plus(new Point(centerX, centerY));
        result.push(vertex.x, vertex.y);
    }
    return result;
}
//# sourceMappingURL=RoundedPolygon.js.map