'use client';

import { CheckCircle, AlertTriangle, XCircle } from 'lucide-react';

interface ConfidenceBadgeProps {
  confidence: number; // 0.0-1.0 scale
  showLabel?: boolean;
  size?: 'sm' | 'md' | 'lg';
}

/**
 * Confidence badge component for displaying OCR extraction confidence
 *
 * Confidence thresholds:
 * - High (≥85%): Green - Ready for processing
 * - Medium (70-84%): Yellow - Review recommended
 * - Low (<70%): Red - Manual intervention needed
 */
export function ConfidenceBadge({
  confidence,
  showLabel = true,
  size = 'md',
}: ConfidenceBadgeProps) {
  // Normalize confidence to 0-1 range if passed as percentage
  const normalizedConf = confidence > 1 ? confidence / 100 : confidence;
  const percentage = Math.round(normalizedConf * 100);

  const getConfig = (conf: number) => {
    if (conf >= 0.85) {
      return {
        color: 'bg-green-100 text-green-800 border-green-300 dark:bg-green-900/30 dark:text-green-300 dark:border-green-700',
        label: 'High',
        icon: CheckCircle,
        description: 'Ready for processing',
      };
    }
    if (conf >= 0.70) {
      return {
        color: 'bg-yellow-100 text-yellow-800 border-yellow-300 dark:bg-yellow-900/30 dark:text-yellow-300 dark:border-yellow-700',
        label: 'Medium',
        icon: AlertTriangle,
        description: 'Review recommended',
      };
    }
    return {
      color: 'bg-red-100 text-red-800 border-red-300 dark:bg-red-900/30 dark:text-red-300 dark:border-red-700',
      label: 'Low',
      icon: XCircle,
      description: 'Manual intervention needed',
    };
  };

  const sizeClasses = {
    sm: 'px-2 py-0.5 text-xs',
    md: 'px-2.5 py-1 text-sm',
    lg: 'px-3 py-1.5 text-base',
  };

  const iconSizes = {
    sm: 'w-3 h-3',
    md: 'w-4 h-4',
    lg: 'w-5 h-5',
  };

  const config = getConfig(normalizedConf);
  const Icon = config.icon;

  return (
    <div className="flex items-center gap-1.5">
      <span
        className={`inline-flex items-center gap-1 rounded-full border font-medium ${config.color} ${sizeClasses[size]}`}
        title={`${config.description} - ${percentage}% confidence`}
      >
        <Icon className={iconSizes[size]} />
        {showLabel && <span>{config.label}</span>}
        <span>{percentage}%</span>
      </span>
    </div>
  );
}

/**
 * Get confidence level description
 */
export function getConfidenceDescription(confidence: number): string {
  const normalizedConf = confidence > 1 ? confidence / 100 : confidence;

  if (normalizedConf >= 0.85) {
    return 'OCR extraction is highly confident. Invoice is ready for processing.';
  }
  if (normalizedConf >= 0.70) {
    return 'OCR extraction has medium confidence. Manual review recommended before processing.';
  }
  return 'OCR extraction has low confidence. Manual intervention required to verify invoice data.';
}

/**
 * Get confidence level for styling
 */
export function getConfidenceLevel(
  confidence: number
): 'high' | 'medium' | 'low' {
  const normalizedConf = confidence > 1 ? confidence / 100 : confidence;

  if (normalizedConf >= 0.85) return 'high';
  if (normalizedConf >= 0.70) return 'medium';
  return 'low';
}
