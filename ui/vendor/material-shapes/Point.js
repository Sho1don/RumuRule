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
function interpolate(start, stop, fraction) {
    return start + (stop - start) * fraction;
}
export class Point {
    x;
    y;
    constructor(x, y) {
        this.x = x;
        this.y = y;
    }
    static interpolate(start, stop, fraction) {
        return new Point(interpolate(start.x, stop.x, fraction), interpolate(start.y, stop.y, fraction));
    }
    copy(x = this.x, y = this.y) {
        return new Point(x, y);
    }
    getDistance() {
        return Math.sqrt(this.x * this.x + this.y * this.y);
    }
    getDistanceSquared() {
        return this.x * this.x + this.y * this.y;
    }
    dotProduct(otherOrX, otherY) {
        if (otherOrX instanceof Point) {
            return this.x * otherOrX.x + this.y * otherOrX.y;
        }
        else {
            return this.x * otherOrX + this.y * otherY;
        }
    }
    clockwise(other) {
        return this.x * other.y - this.y * other.x > 0;
    }
    getDirection() {
        const d = this.getDistance();
        if (d <= 0) {
            throw new Error("Can't get the direction of a 0-length vector");
        }
        return this.div(d);
    }
    unaryMinus() {
        return new Point(-this.x, -this.y);
    }
    minus(other) {
        return new Point(this.x - other.x, this.y - other.y);
    }
    plus(other) {
        return new Point(this.x + other.x, this.y + other.y);
    }
    times(operand) {
        return new Point(this.x * operand, this.y * operand);
    }
    div(operand) {
        return new Point(this.x / operand, this.y / operand);
    }
    rem(operand) {
        return new Point(this.x % operand, this.y % operand);
    }
    transformed(f) {
        const result = f.transform(this.x, this.y);
        return new Point(result.first, result.second);
    }
    rotate90() {
        return new Point(-this.y, this.x);
    }
}
//# sourceMappingURL=Point.js.map