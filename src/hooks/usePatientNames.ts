/**
 * usePatientNames.ts — Resolves an array of patient IDs to display names.
 *
 * Returns a Map<patientId, displayName> that updates as patients are fetched.
 * Caches results so repeated renders don't trigger redundant API calls.
 */
import { useState, useEffect, useRef } from "react";
import { commands } from "../lib/tauri";
import { extractPatientDisplay } from "../lib/fhirExtract";

/**
 * Given an array of patient IDs, resolve each to a display name.
 * Returns a stable Map that grows as results arrive.
 */
export function usePatientNames(patientIds: string[]): Map<string, string> {
  const [nameMap, setNameMap] = useState<Map<string, string>>(new Map());
  // Cache across re-renders to avoid refetching known IDs.
  const cacheRef = useRef<Map<string, string>>(new Map());

  useEffect(() => {
    let mounted = true;

    // Find IDs we haven't resolved yet.
    const unknownIds = patientIds.filter((id) => id && !cacheRef.current.has(id));
    if (unknownIds.length === 0) {
      // All known — just sync state from cache.
      const next = new Map<string, string>();
      for (const id of patientIds) {
        const name = cacheRef.current.get(id);
        if (name) next.set(id, name);
      }
      setNameMap(next);
      return;
    }

    // Fetch unknown patients in parallel.
    const dedupedIds = [...new Set(unknownIds)];

    Promise.allSettled(
      dedupedIds.map(async (id) => {
        try {
          const record = await commands.getPatient(id);
          const display = extractPatientDisplay(
            record.resource as Record<string, unknown>,
          );
          const given = display.givenNames.join(" ");
          const family = display.familyName ?? "";
          const fullName = `${given} ${family}`.trim();
          return { id, name: fullName || id };
        } catch {
          return { id, name: id };
        }
      }),
    ).then((results) => {
      if (!mounted) return;
      for (const r of results) {
        if (r.status === "fulfilled") {
          cacheRef.current.set(r.value.id, r.value.name);
        }
      }
      const next = new Map<string, string>();
      for (const id of patientIds) {
        const name = cacheRef.current.get(id);
        if (name) next.set(id, name);
      }
      setNameMap(next);
    });

    return () => {
      mounted = false;
    };
  }, [patientIds.join(",")]); // eslint-disable-line react-hooks/exhaustive-deps

  return nameMap;
}

/**
 * Convert a full name to initials. "Omar Sharaf" → "OS".
 * Handles multi-word names: "Omar Safwat Sharaf" → "OSS".
 * Falls back to first two chars if name has no spaces.
 */
export function toInitials(name: string): string {
  if (!name) return "";
  const parts = name.trim().split(/\s+/);
  if (parts.length === 1) {
    // Single word — return first 2 chars uppercased
    return name.slice(0, 2).toUpperCase();
  }
  return parts.map((p) => p.charAt(0).toUpperCase()).join("");
}
