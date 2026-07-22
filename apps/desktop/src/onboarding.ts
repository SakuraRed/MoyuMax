import type { OnboardingSelection } from "./runtime";

export type OnboardingStep = "language" | "data" | "privacy" | "done";

export interface OnboardingState {
  step: OnboardingStep;
  draft: OnboardingSelection;
  isSubmitting: boolean;
  errorMessage: string | null;
}

const NEXT_STEP: Readonly<Record<OnboardingStep, OnboardingStep>> = {
  language: "data",
  data: "privacy",
  privacy: "done",
  done: "done",
};

const PREVIOUS_STEP: Readonly<Record<OnboardingStep, OnboardingStep>> = {
  language: "language",
  data: "language",
  privacy: "data",
  done: "privacy",
};

export function createOnboardingState(
  defaults: OnboardingSelection,
): OnboardingState {
  return {
    step: "language",
    draft: { ...defaults },
    isSubmitting: false,
    errorMessage: null,
  };
}

export function goForward(state: OnboardingState): OnboardingState {
  return {
    ...state,
    step: NEXT_STEP[state.step],
    errorMessage: null,
  };
}

export function goBack(state: OnboardingState): OnboardingState {
  return {
    ...state,
    step: PREVIOUS_STEP[state.step],
    errorMessage: null,
  };
}

export function updateOnboardingDraft(
  state: OnboardingState,
  change: Partial<OnboardingSelection>,
): OnboardingState {
  return {
    ...state,
    draft: {
      ...state.draft,
      ...change,
    },
    errorMessage: null,
  };
}

export function setSubmitting(
  state: OnboardingState,
  isSubmitting: boolean,
): OnboardingState {
  return { ...state, isSubmitting, errorMessage: null };
}

export function setOnboardingError(
  state: OnboardingState,
  errorMessage: string,
): OnboardingState {
  return { ...state, isSubmitting: false, errorMessage };
}

export function buildSelection(
  state: OnboardingState,
): OnboardingSelection {
  return { ...state.draft };
}
