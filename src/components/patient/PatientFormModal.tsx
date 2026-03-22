/**
 * PatientFormModal.tsx — Create / Edit patient form modal.
 *
 * Two-tab form:
 *   Tab 1 "Basic Info"         — demographics (name, DOB, gender, contact, address)
 *   Tab 2 "Insurance & Other"  — primary insurance + care team fields
 *
 * Create path:  commands.createPatient → optional commands.upsertCareTeam → onSuccess(id)
 * Edit path:    commands.updatePatient → optional commands.upsertCareTeam → onSuccess(id)
 *
 * Validation:
 *   - familyName is required.
 *   - If ANY care team field is filled, ALL of ctMemberId, ctMemberName, ctRole
 *     must be filled (per upsertCareTeam non-nullable constraint).
 *
 * Pre-population: initialDisplay initializes form state at mount (useState initializer),
 * not a useEffect, so there is no empty-field flash.
 *
 * Observability: submitError is rendered inline above the submit button — visible
 * without DevTools. submitting and submitError are inspectable in React DevTools.
 *
 * Modal overlay pattern: position: fixed inset-0 z-50 (same as LockScreen.tsx).
 * Input / label Tailwind classes mirror LoginForm.tsx exactly.
 */
import { useState, useEffect, type FormEvent } from "react";
import { commands } from "../../lib/tauri";
import type { PatientDisplay } from "../../lib/fhirExtract";
import type { PatientInput, InsuranceInput, CareTeamMemberInput } from "../../types/patient";

// ─── Props ────────────────────────────────────────────────────────────────────

export interface PatientFormModalProps {
  /** When provided, this is an edit session. When null, it is a create session. */
  patientId: string | null;
  /** Pre-populated from extractPatientDisplay when editing. Null for new patient. */
  initialDisplay: PatientDisplay | null;
  /** Called after successful create/update so the parent can reload or navigate. */
  onSuccess: (patientId: string) => void;
  /** Called when the user cancels. */
  onClose: () => void;
  /**
   * Called when the user confirms patient deletion (SystemAdmin only).
   * When undefined, the delete section is not shown.
   */
  onDelete?: () => Promise<void>;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";
const ERROR_CLS = "mt-1 text-xs text-red-600";

function FormField({
  label,
  htmlFor,
  error,
  children,
}: {
  label: string;
  htmlFor: string;
  error?: string | null;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label htmlFor={htmlFor} className={LABEL_CLS}>
        {label}
      </label>
      {children}
      {error && <p className={ERROR_CLS}>{error}</p>}
    </div>
  );
}

// ─── Component ────────────────────────────────────────────────────────────────

export function PatientFormModal({
  patientId,
  initialDisplay,
  onSuccess,
  onClose,
  onDelete,
}: PatientFormModalProps) {
  const isEdit = patientId !== null;

  // ── Tab ────────────────────────────────────────────────────────────────
  const [activeTab, setActiveTab] = useState<"basic" | "insurance">("basic");

  // ── Basic Info ─────────────────────────────────────────────────────────
  const [familyName, setFamilyName] = useState(initialDisplay?.familyName ?? "");
  const [givenName, setGivenName] = useState(
    initialDisplay?.givenNames?.join(" ") ?? ""
  );
  const [birthDate, setBirthDate] = useState(initialDisplay?.dob ?? "");
  const [gender, setGender] = useState(initialDisplay?.gender ?? "");
  const [phone, setPhone] = useState(initialDisplay?.phone ?? "");
  const [email, setEmail] = useState(initialDisplay?.email ?? "");
  const [addressLine, setAddressLine] = useState(initialDisplay?.addressLine ?? "");
  const [city, setCity] = useState(initialDisplay?.city ?? "");
  const [state, setState] = useState(initialDisplay?.state ?? "");
  const [postalCode, setPostalCode] = useState(initialDisplay?.postalCode ?? "");

  // ── Insurance & Other ──────────────────────────────────────────────────
  const [payerName, setPayerName] = useState(
    initialDisplay?.insurancePrimary?.payerName ?? ""
  );
  const [memberId, setMemberId] = useState(
    initialDisplay?.insurancePrimary?.memberId ?? ""
  );
  const [planName, setPlanName] = useState(
    initialDisplay?.insurancePrimary?.planName ?? ""
  );
  const [groupNumber, setGroupNumber] = useState(
    initialDisplay?.insurancePrimary?.groupNumber ?? ""
  );

  // Care team fields
  const [ctMemberId, setCtMemberId] = useState("");
  const [ctMemberName, setCtMemberName] = useState("");
  const [ctRole, setCtRole] = useState("");
  const [ctNote, setCtNote] = useState("");

  // ── Submission state ───────────────────────────────────────────────────
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // ── Inline validation errors ───────────────────────────────────────────
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});

  // ── Delete patient state ────────────────────────────────────────────────
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [deletingPatient, setDeletingPatient] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  // ── Reset form when switching patients while modal is open ────────────
  useEffect(() => {
    setFamilyName(initialDisplay?.familyName ?? "");
    setGivenName(initialDisplay?.givenNames?.join(" ") ?? "");
    setBirthDate(initialDisplay?.dob ?? "");
    setGender(initialDisplay?.gender ?? "");
    setPhone(initialDisplay?.phone ?? "");
    setEmail(initialDisplay?.email ?? "");
    setAddressLine(initialDisplay?.addressLine ?? "");
    setCity(initialDisplay?.city ?? "");
    setState(initialDisplay?.state ?? "");
    setPostalCode(initialDisplay?.postalCode ?? "");
    setPayerName(initialDisplay?.insurancePrimary?.payerName ?? "");
    setMemberId(initialDisplay?.insurancePrimary?.memberId ?? "");
    setPlanName(initialDisplay?.insurancePrimary?.planName ?? "");
    setGroupNumber(initialDisplay?.insurancePrimary?.groupNumber ?? "");
    setCtMemberId("");
    setCtMemberName("");
    setCtRole("");
    setCtNote("");
    setSubmitError(null);
    setFieldErrors({});
    setActiveTab("basic");
  }, [patientId]);

  // ── Submit ─────────────────────────────────────────────────────────────

  function validate(): boolean {
    const errors: Record<string, string> = {};

    if (!familyName.trim()) {
      errors["familyName"] = "Last name is required.";
    }

    // Care team: all-or-nothing rule
    const careTeamPartial =
      ctMemberId.trim() || ctMemberName.trim() || ctRole.trim();
    if (careTeamPartial) {
      if (!ctMemberId.trim())
        errors["ctMemberId"] = "Member ID is required when adding a care team member.";
      if (!ctMemberName.trim())
        errors["ctMemberName"] = "Member name is required when adding a care team member.";
      if (!ctRole.trim())
        errors["ctRole"] = "Role is required when adding a care team member.";
    }

    setFieldErrors(errors);
    return Object.keys(errors).length === 0;
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!validate()) return;

    setSubmitting(true);
    setSubmitError(null);

    try {
      // Build insurance input (primary only for S02 MVP)
      const insurancePrimary: InsuranceInput | null = payerName.trim()
        ? {
            payerName: payerName.trim(),
            memberId: memberId.trim(),
            planName: planName.trim() || null,
            groupNumber: groupNumber.trim() || null,
            subscriberName: null,
            subscriberDob: null,
            relationshipToSubscriber: null,
          }
        : null;

      const input: PatientInput = {
        familyName: familyName.trim(),
        // givenNames is always a single-element array — never split on whitespace
        givenNames: [givenName.trim()],
        birthDate: birthDate.trim() || null,
        gender: gender.trim() || null,
        genderIdentity: null,
        phone: phone.trim() || null,
        email: email.trim() || null,
        addressLine: addressLine.trim() || null,
        city: city.trim() || null,
        state: state.trim() || null,
        postalCode: postalCode.trim() || null,
        country: null,
        photoUrl: null,
        mrn: null,
        primaryProviderId: null,
        insurancePrimary,
        insuranceSecondary: null,
        insuranceTertiary: null,
        employer: null,
        sdoh: null,
      };

      let resolvedPatientId: string;

      if (isEdit) {
        // Edit path
        await commands.updatePatient(patientId, input);
        resolvedPatientId = patientId;
      } else {
        // Create path
        const record = await commands.createPatient(input);
        resolvedPatientId = record.id;
      }

      // Optional care team upsert — only when all required fields are filled
      const careTeamFilled =
        ctMemberId.trim() && ctMemberName.trim() && ctRole.trim();
      if (careTeamFilled) {
        const careTeamInput: CareTeamMemberInput = {
          patientId: resolvedPatientId,
          memberId: ctMemberId.trim(),
          memberName: ctMemberName.trim(),
          role: ctRole.trim(),
          note: ctNote.trim() || null,
        };
        await commands.upsertCareTeam(careTeamInput);
      }

      onSuccess(resolvedPatientId);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    /* Backdrop — same fixed inset-0 z-50 pattern as LockScreen.tsx */
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 overflow-y-auto">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-2xl mx-auto mt-16 mb-16 p-6">
        {/* ── Modal header ──────────────────────────────────────────── */}
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-lg font-semibold text-gray-900">
            {isEdit ? "Edit Patient" : "New Patient"}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-600 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
            aria-label="Close"
          >
            ✕
          </button>
        </div>

        {/* ── Tab bar ───────────────────────────────────────────────── */}
        <div className="mb-5 flex gap-1 border-b border-gray-200">
          <button
            type="button"
            onClick={() => setActiveTab("basic")}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors focus:outline-none ${
              activeTab === "basic"
                ? "border-blue-600 text-blue-600"
                : "border-transparent text-gray-500 hover:text-gray-700"
            }`}
          >
            Basic Info
          </button>
          <button
            type="button"
            onClick={() => setActiveTab("insurance")}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors focus:outline-none ${
              activeTab === "insurance"
                ? "border-blue-600 text-blue-600"
                : "border-transparent text-gray-500 hover:text-gray-700"
            }`}
          >
            Insurance &amp; Other
          </button>
        </div>

        {/* ── Form ──────────────────────────────────────────────────── */}
        <form onSubmit={handleSubmit} noValidate>
          {/* Tab 1: Basic Info */}
          {activeTab === "basic" && (
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <FormField label="First Name" htmlFor="givenName">
                  <input
                    id="givenName"
                    type="text"
                    value={givenName}
                    onChange={(e) => setGivenName(e.target.value)}
                    className={INPUT_CLS}
                    autoComplete="given-name"
                    autoFocus
                  />
                </FormField>

                <FormField
                  label="Last Name *"
                  htmlFor="familyName"
                  error={fieldErrors["familyName"]}
                >
                  <input
                    id="familyName"
                    type="text"
                    value={familyName}
                    onChange={(e) => setFamilyName(e.target.value)}
                    className={INPUT_CLS}
                    autoComplete="family-name"
                  />
                </FormField>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <FormField label="Date of Birth" htmlFor="birthDate">
                  <input
                    id="birthDate"
                    type="date"
                    value={birthDate}
                    onChange={(e) => setBirthDate(e.target.value)}
                    className={INPUT_CLS}
                  />
                </FormField>

                <FormField label="Gender" htmlFor="gender">
                  <select
                    id="gender"
                    value={gender}
                    onChange={(e) => setGender(e.target.value)}
                    className={INPUT_CLS}
                  >
                    <option value="">— select —</option>
                    <option value="male">Male</option>
                    <option value="female">Female</option>
                    <option value="other">Other</option>
                    <option value="unknown">Unknown</option>
                  </select>
                </FormField>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <FormField label="Phone" htmlFor="phone">
                  <input
                    id="phone"
                    type="tel"
                    value={phone}
                    onChange={(e) => setPhone(e.target.value)}
                    className={INPUT_CLS}
                    autoComplete="tel"
                  />
                </FormField>

                <FormField label="Email" htmlFor="email">
                  <input
                    id="email"
                    type="email"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    className={INPUT_CLS}
                    autoComplete="email"
                  />
                </FormField>
              </div>

              <FormField label="Street Address" htmlFor="addressLine">
                <input
                  id="addressLine"
                  type="text"
                  value={addressLine}
                  onChange={(e) => setAddressLine(e.target.value)}
                  className={INPUT_CLS}
                  autoComplete="street-address"
                />
              </FormField>

              <div className="grid grid-cols-3 gap-4">
                <FormField label="City" htmlFor="city">
                  <input
                    id="city"
                    type="text"
                    value={city}
                    onChange={(e) => setCity(e.target.value)}
                    className={INPUT_CLS}
                    autoComplete="address-level2"
                  />
                </FormField>

                <FormField label="State" htmlFor="state">
                  <input
                    id="state"
                    type="text"
                    value={state}
                    onChange={(e) => setState(e.target.value)}
                    className={INPUT_CLS}
                    autoComplete="address-level1"
                    maxLength={2}
                    placeholder="e.g. CA"
                  />
                </FormField>

                <FormField label="Postal Code" htmlFor="postalCode">
                  <input
                    id="postalCode"
                    type="text"
                    value={postalCode}
                    onChange={(e) => setPostalCode(e.target.value)}
                    className={INPUT_CLS}
                    autoComplete="postal-code"
                  />
                </FormField>
              </div>
            </div>
          )}

          {/* Tab 2: Insurance & Other */}
          {activeTab === "insurance" && (
            <div className="space-y-6">
              {/* Primary insurance */}
              <div>
                <h3 className="mb-3 text-sm font-semibold text-gray-700 uppercase tracking-wide">
                  Primary Insurance
                </h3>
                <div className="space-y-4">
                  <div className="grid grid-cols-2 gap-4">
                    <FormField label="Payer Name" htmlFor="payerName">
                      <input
                        id="payerName"
                        type="text"
                        value={payerName}
                        onChange={(e) => setPayerName(e.target.value)}
                        className={INPUT_CLS}
                        placeholder="e.g. Blue Cross"
                      />
                    </FormField>

                    <FormField label="Member ID" htmlFor="memberId">
                      <input
                        id="memberId"
                        type="text"
                        value={memberId}
                        onChange={(e) => setMemberId(e.target.value)}
                        className={INPUT_CLS}
                      />
                    </FormField>
                  </div>

                  <div className="grid grid-cols-2 gap-4">
                    <FormField label="Plan Name" htmlFor="planName">
                      <input
                        id="planName"
                        type="text"
                        value={planName}
                        onChange={(e) => setPlanName(e.target.value)}
                        className={INPUT_CLS}
                      />
                    </FormField>

                    <FormField label="Group Number" htmlFor="groupNumber">
                      <input
                        id="groupNumber"
                        type="text"
                        value={groupNumber}
                        onChange={(e) => setGroupNumber(e.target.value)}
                        className={INPUT_CLS}
                      />
                    </FormField>
                  </div>
                </div>
              </div>

              {/* Care team */}
              <div>
                <h3 className="mb-3 text-sm font-semibold text-gray-700 uppercase tracking-wide">
                  Care Team Member
                </h3>
                <p className="mb-3 text-xs text-gray-500">
                  All three required fields (Member ID, Name, Role) must be
                  filled together, or leave all blank.
                </p>
                <div className="space-y-4">
                  <div className="grid grid-cols-2 gap-4">
                    <FormField
                      label="Member ID"
                      htmlFor="ctMemberId"
                      error={fieldErrors["ctMemberId"]}
                    >
                      <input
                        id="ctMemberId"
                        type="text"
                        value={ctMemberId}
                        onChange={(e) => setCtMemberId(e.target.value)}
                        className={INPUT_CLS}
                        placeholder="Provider user ID"
                      />
                    </FormField>

                    <FormField
                      label="Member Name"
                      htmlFor="ctMemberName"
                      error={fieldErrors["ctMemberName"]}
                    >
                      <input
                        id="ctMemberName"
                        type="text"
                        value={ctMemberName}
                        onChange={(e) => setCtMemberName(e.target.value)}
                        className={INPUT_CLS}
                        placeholder="Display name"
                      />
                    </FormField>
                  </div>

                  <div className="grid grid-cols-2 gap-4">
                    <FormField
                      label="Role"
                      htmlFor="ctRole"
                      error={fieldErrors["ctRole"]}
                    >
                      <input
                        id="ctRole"
                        type="text"
                        value={ctRole}
                        onChange={(e) => setCtRole(e.target.value)}
                        className={INPUT_CLS}
                        placeholder="e.g. primary_care"
                      />
                    </FormField>

                    <FormField label="Note (optional)" htmlFor="ctNote">
                      <input
                        id="ctNote"
                        type="text"
                        value={ctNote}
                        onChange={(e) => setCtNote(e.target.value)}
                        className={INPUT_CLS}
                      />
                    </FormField>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* ── Submit error ───────────────────────────────────────── */}
          {submitError && (
            <div className="mt-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
              <p className="font-semibold">Failed to save patient</p>
              <p className="mt-0.5">{submitError}</p>
            </div>
          )}

          {/* ── Footer ────────────────────────────────────────────── */}
          <div className="mt-6 flex items-center justify-between gap-3 border-t border-gray-100 pt-4">
            {/* Left side: Delete button (only in edit mode, only when onDelete provided) */}
            <div>
              {isEdit && onDelete && !showDeleteConfirm && (
                <button
                  type="button"
                  onClick={() => setShowDeleteConfirm(true)}
                  disabled={submitting || deletingPatient}
                  className="rounded-md border border-red-300 bg-white px-4 py-2 text-sm font-medium text-red-700 shadow-sm hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
                >
                  Delete Patient
                </button>
              )}
            </div>

            {/* Right side: Cancel + Save */}
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={onClose}
                disabled={submitting || deletingPatient}
                className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={submitting || deletingPatient}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
              >
                {submitting
                  ? isEdit
                    ? "Saving…"
                    : "Creating…"
                  : isEdit
                    ? "Save Changes"
                    : "Create Patient"}
              </button>
            </div>
          </div>

          {/* ── Delete Patient confirmation ────────────────────────── */}
          {isEdit && onDelete && showDeleteConfirm && (
            <div className="mt-4 rounded-lg border border-red-300 bg-red-50 p-4">
              <h4 className="text-sm font-semibold text-red-800">Delete Patient</h4>
              <p className="mt-1 text-sm text-red-700">
                Are you sure? This cannot be undone. All patient data including
                encounters, appointments, and documents will be permanently deleted.
              </p>
              {deleteError && (
                <div className="mt-2 rounded-md border border-red-200 bg-white px-3 py-2 text-sm text-red-700">
                  {deleteError}
                </div>
              )}
              <div className="mt-3 flex gap-3">
                <button
                  type="button"
                  onClick={async () => {
                    setDeletingPatient(true);
                    setDeleteError(null);
                    try {
                      await onDelete();
                      // onDelete navigates away, so no cleanup needed
                    } catch (e) {
                      const msg = e instanceof Error ? e.message : String(e);
                      setDeleteError(msg);
                      setDeletingPatient(false);
                    }
                  }}
                  disabled={deletingPatient}
                  className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-red-700 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
                >
                  {deletingPatient ? "Deleting..." : "Confirm Delete"}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setShowDeleteConfirm(false);
                    setDeleteError(null);
                  }}
                  disabled={deletingPatient}
                  className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2"
                >
                  Cancel
                </button>
              </div>
            </div>
          )}
        </form>
      </div>
    </div>
  );
}
