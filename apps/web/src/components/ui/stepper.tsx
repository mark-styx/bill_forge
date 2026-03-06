'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import { Check, ChevronLeft, ChevronRight, Loader2 } from 'lucide-react';
import { Button } from './button';

export interface Step {
  id: string;
  title: string;
  description?: string;
  icon?: React.ReactNode;
  optional?: boolean;
  completed?: boolean;
  error?: boolean;
}

export interface StepperProps {
  steps: Step[];
  currentStep: number;
  onStepChange?: (step: number) => void;
  orientation?: 'horizontal' | 'vertical';
  variant?: 'default' | 'dots' | 'line' | 'numbered';
  size?: 'sm' | 'default' | 'lg';
  clickable?: boolean;
  showLabels?: boolean;
  className?: string;
}

export function Stepper({
  steps,
  currentStep,
  onStepChange,
  orientation = 'horizontal',
  variant = 'default',
  size = 'default',
  clickable = false,
  showLabels = true,
  className,
}: StepperProps) {
  const sizeClasses = {
    sm: { icon: 'w-6 h-6 text-xs', connector: 'h-0.5' },
    default: { icon: 'w-8 h-8 text-sm', connector: 'h-0.5' },
    lg: { icon: 'w-10 h-10 text-base', connector: 'h-1' },
  };

  const sizes = sizeClasses[size];

  const handleStepClick = (index: number) => {
    if (clickable && onStepChange) {
      onStepChange(index);
    }
  };

  // Dots variant
  if (variant === 'dots') {
    return (
      <div className={cn('flex items-center gap-2', className)}>
        {steps.map((step, index) => {
          const isActive = index === currentStep;
          const isCompleted = index < currentStep || step.completed;

          return (
            <button
              key={step.id}
              onClick={() => handleStepClick(index)}
              disabled={!clickable}
              className={cn(
                'rounded-full transition-all',
                size === 'sm' && 'w-2 h-2',
                size === 'default' && 'w-2.5 h-2.5',
                size === 'lg' && 'w-3 h-3',
                isActive && 'bg-primary scale-125',
                isCompleted && !isActive && 'bg-primary/50',
                !isActive && !isCompleted && 'bg-border',
                clickable && 'cursor-pointer hover:scale-110'
              )}
              title={step.title}
            />
          );
        })}
      </div>
    );
  }

  // Line variant
  if (variant === 'line') {
    const progress = (currentStep / (steps.length - 1)) * 100;
    return (
      <div className={cn('relative', className)}>
        {/* Background line */}
        <div className={cn('absolute top-0 left-0 right-0 bg-border rounded-full', sizes.connector)} />
        {/* Progress line */}
        <div
          className={cn('absolute top-0 left-0 bg-primary rounded-full transition-all duration-300', sizes.connector)}
          style={{ width: `${progress}%` }}
        />
        {/* Step labels */}
        {showLabels && (
          <div className="flex justify-between mt-3">
            {steps.map((step, index) => {
              const isActive = index === currentStep;
              const isCompleted = index < currentStep || step.completed;
              return (
                <button
                  key={step.id}
                  onClick={() => handleStepClick(index)}
                  disabled={!clickable}
                  className={cn(
                    'text-xs font-medium transition-colors',
                    isActive && 'text-primary',
                    isCompleted && !isActive && 'text-foreground',
                    !isActive && !isCompleted && 'text-muted-foreground',
                    clickable && 'cursor-pointer hover:text-primary'
                  )}
                >
                  {step.title}
                </button>
              );
            })}
          </div>
        )}
      </div>
    );
  }

  // Numbered variant
  if (variant === 'numbered') {
    return (
      <div
        className={cn(
          'flex',
          orientation === 'horizontal' ? 'items-center' : 'flex-col',
          className
        )}
      >
        {steps.map((step, index) => {
          const isActive = index === currentStep;
          const isCompleted = index < currentStep || step.completed;
          const isLast = index === steps.length - 1;

          return (
            <React.Fragment key={step.id}>
              <div
                className={cn(
                  'flex items-center',
                  orientation === 'vertical' && 'gap-3'
                )}
              >
                <button
                  onClick={() => handleStepClick(index)}
                  disabled={!clickable}
                  className={cn(
                    'rounded-full flex items-center justify-center font-semibold transition-all',
                    sizes.icon,
                    isActive && 'bg-primary text-primary-foreground shadow-md',
                    isCompleted && !isActive && 'bg-primary text-primary-foreground',
                    !isActive && !isCompleted && 'bg-secondary text-muted-foreground',
                    step.error && 'bg-error text-error-foreground',
                    clickable && 'cursor-pointer hover:scale-105'
                  )}
                >
                  {isCompleted && !isActive ? (
                    <Check className="w-4 h-4" />
                  ) : (
                    index + 1
                  )}
                </button>

                {showLabels && orientation === 'vertical' && (
                  <div className="flex-1">
                    <p
                      className={cn(
                        'font-medium',
                        isActive && 'text-foreground',
                        !isActive && 'text-muted-foreground'
                      )}
                    >
                      {step.title}
                    </p>
                    {step.description && (
                      <p className="text-sm text-muted-foreground">
                        {step.description}
                      </p>
                    )}
                  </div>
                )}
              </div>

              {!isLast && (
                <div
                  className={cn(
                    'bg-border',
                    orientation === 'horizontal'
                      ? 'flex-1 h-0.5 mx-3'
                      : 'w-0.5 h-6 ml-4 my-1',
                    isCompleted && 'bg-primary'
                  )}
                />
              )}
            </React.Fragment>
          );
        })}
      </div>
    );
  }

  // Default variant
  return (
    <div
      className={cn(
        'flex',
        orientation === 'horizontal' ? 'items-start' : 'flex-col',
        className
      )}
    >
      {steps.map((step, index) => {
        const isActive = index === currentStep;
        const isCompleted = index < currentStep || step.completed;
        const isLast = index === steps.length - 1;

        return (
          <React.Fragment key={step.id}>
            <div
              className={cn(
                'flex',
                orientation === 'horizontal' ? 'flex-col items-center' : 'items-start gap-3'
              )}
            >
              {/* Step indicator */}
              <button
                onClick={() => handleStepClick(index)}
                disabled={!clickable}
                className={cn(
                  'rounded-full flex items-center justify-center transition-all border-2',
                  sizes.icon,
                  isActive && 'border-primary bg-primary text-primary-foreground shadow-lg scale-110',
                  isCompleted && !isActive && 'border-primary bg-primary text-primary-foreground',
                  !isActive && !isCompleted && 'border-border bg-background text-muted-foreground',
                  step.error && 'border-error bg-error text-error-foreground',
                  clickable && !isActive && 'cursor-pointer hover:border-primary/70 hover:scale-105'
                )}
              >
                {isCompleted && !isActive ? (
                  <Check className="w-4 h-4" />
                ) : step.icon ? (
                  step.icon
                ) : (
                  <span className="font-medium">{index + 1}</span>
                )}
              </button>

              {/* Step content */}
              {showLabels && (
                <div
                  className={cn(
                    orientation === 'horizontal' ? 'text-center mt-2' : 'flex-1'
                  )}
                >
                  <p
                    className={cn(
                      'font-medium text-sm',
                      isActive && 'text-foreground',
                      !isActive && 'text-muted-foreground'
                    )}
                  >
                    {step.title}
                    {step.optional && (
                      <span className="text-xs text-muted-foreground ml-1">(Optional)</span>
                    )}
                  </p>
                  {step.description && orientation === 'vertical' && (
                    <p className="text-sm text-muted-foreground mt-0.5">
                      {step.description}
                    </p>
                  )}
                </div>
              )}
            </div>

            {/* Connector */}
            {!isLast && (
              <div
                className={cn(
                  orientation === 'horizontal' ? 'flex-1 h-0.5 mx-2 mt-4' : 'w-0.5 h-8 ml-4',
                  isCompleted ? 'bg-primary' : 'bg-border'
                )}
              />
            )}
          </React.Fragment>
        );
      })}
    </div>
  );
}

// Stepper with content
export interface StepperWithContentProps extends Omit<StepperProps, 'currentStep' | 'onStepChange'> {
  children: React.ReactNode;
  initialStep?: number;
  onComplete?: () => void;
  onStepComplete?: (stepIndex: number) => Promise<boolean> | boolean;
  showNavigation?: boolean;
  nextLabel?: string;
  prevLabel?: string;
  completeLabel?: string;
  isLoading?: boolean;
}

export function StepperWithContent({
  steps,
  children,
  initialStep = 0,
  onComplete,
  onStepComplete,
  showNavigation = true,
  nextLabel = 'Next',
  prevLabel = 'Back',
  completeLabel = 'Complete',
  isLoading = false,
  ...stepperProps
}: StepperWithContentProps) {
  const [currentStep, setCurrentStep] = React.useState(initialStep);
  const [isNavigating, setIsNavigating] = React.useState(false);

  const childrenArray = React.Children.toArray(children);
  const currentContent = childrenArray[currentStep];

  const isFirstStep = currentStep === 0;
  const isLastStep = currentStep === steps.length - 1;

  const handleNext = async () => {
    if (isNavigating) return;

    setIsNavigating(true);
    try {
      if (onStepComplete) {
        const canProceed = await onStepComplete(currentStep);
        if (!canProceed) {
          setIsNavigating(false);
          return;
        }
      }

      if (isLastStep) {
        onComplete?.();
      } else {
        setCurrentStep((prev) => Math.min(prev + 1, steps.length - 1));
      }
    } finally {
      setIsNavigating(false);
    }
  };

  const handlePrev = () => {
    setCurrentStep((prev) => Math.max(prev - 1, 0));
  };

  return (
    <div className="space-y-6">
      <Stepper
        steps={steps}
        currentStep={currentStep}
        onStepChange={setCurrentStep}
        {...stepperProps}
      />

      <div className="min-h-[200px]">{currentContent}</div>

      {showNavigation && (
        <div className="flex items-center justify-between pt-4 border-t border-border">
          <Button
            variant="outline"
            onClick={handlePrev}
            disabled={isFirstStep || isNavigating || isLoading}
          >
            <ChevronLeft className="w-4 h-4 mr-1" />
            {prevLabel}
          </Button>

          <div className="text-sm text-muted-foreground">
            Step {currentStep + 1} of {steps.length}
          </div>

          <Button
            onClick={handleNext}
            disabled={isNavigating || isLoading}
          >
            {(isNavigating || isLoading) && (
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
            )}
            {isLastStep ? completeLabel : nextLabel}
            {!isLastStep && <ChevronRight className="w-4 h-4 ml-1" />}
          </Button>
        </div>
      )}
    </div>
  );
}

// Step content wrapper
export interface StepContentProps {
  children: React.ReactNode;
  className?: string;
}

export function StepContent({ children, className }: StepContentProps) {
  return (
    <div className={cn('animate-fade-in', className)}>
      {children}
    </div>
  );
}
