import { Schema } from "effect";
import { request } from "../../lib/api";

const boundedText = (max: number) =>
  Schema.String.pipe(Schema.filter((value) => Array.from(value).length <= max));
export const CanonicalMACSchema = Schema.String.pipe(
  Schema.pattern(/^(?:[0-9A-F]{2}:){5}[0-9A-F]{2}$/),
);
export const DeviceAnnotationSchema = Schema.Struct({
  label: boundedText(80),
  note: boundedText(1000),
  tags: Schema.Array(boundedText(32)).pipe(Schema.maxItems(8)),
});
export const DeviceAnnotationsSchema = Schema.Struct({
  revision: Schema.Int.pipe(Schema.between(0, Number.MAX_SAFE_INTEGER)),
  devices: Schema.Record({
    key: Schema.String,
    value: DeviceAnnotationSchema,
  }).pipe(
    Schema.filter(
      (value) =>
        Object.keys(value).length <= 256 &&
        Object.keys(value).every((key) =>
          /^(?:[0-9A-F]{2}:){5}[0-9A-F]{2}$/.test(key),
        ),
    ),
  ),
});
export type DeviceAnnotation = typeof DeviceAnnotationSchema.Type;
export type DeviceAnnotations = typeof DeviceAnnotationsSchema.Type;
export interface SaveDeviceAnnotation {
  mac: string;
  label: string;
  note: string;
  tags: readonly string[];
  expectedRevision: number;
}
export const deviceAnnotationsAPI = {
  get: () => request("/devices/annotations", DeviceAnnotationsSchema),
  save: (body: SaveDeviceAnnotation) =>
    request("/devices/annotations", DeviceAnnotationsSchema, {
      method: "POST",
      body,
    }),
};
