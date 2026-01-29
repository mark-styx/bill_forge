'use client';

import { useState, useCallback, useRef, useEffect } from 'react';
import { cn } from '@/lib/utils';
import { Check, Pipette } from 'lucide-react';

interface ColorPickerProps {
  value: string; // HSL string like "210 100% 50%"
  onChange: (value: string) => void;
  label?: string;
  presets?: { name: string; value: string }[];
  className?: string;
}

// Bright, vibrant preset colors
const defaultPresets = [
  { name: 'Blue', value: '210 100% 50%' },
  { name: 'Cyan', value: '190 100% 45%' },
  { name: 'Teal', value: '170 80% 40%' },
  { name: 'Emerald', value: '160 84% 39%' },
  { name: 'Green', value: '142 76% 45%' },
  { name: 'Lime', value: '84 85% 45%' },
  { name: 'Yellow', value: '50 100% 50%' },
  { name: 'Amber', value: '38 92% 50%' },
  { name: 'Orange', value: '25 95% 53%' },
  { name: 'Coral', value: '10 90% 60%' },
  { name: 'Rose', value: '350 90% 60%' },
  { name: 'Pink', value: '330 85% 60%' },
  { name: 'Fuchsia', value: '300 80% 55%' },
  { name: 'Purple', value: '270 75% 55%' },
  { name: 'Violet', value: '260 70% 55%' },
  { name: 'Indigo', value: '245 75% 55%' },
];

function parseHSL(hsl: string): { h: number; s: number; l: number } {
  const parts = hsl.split(' ').map((p) => parseFloat(p.replace('%', '')));
  return { h: parts[0] || 0, s: parts[1] || 0, l: parts[2] || 0 };
}

function formatHSL(h: number, s: number, l: number): string {
  return `${Math.round(h)} ${Math.round(s)}% ${Math.round(l)}%`;
}

export function ColorPicker({
  value,
  onChange,
  label,
  presets = defaultPresets,
  className,
}: ColorPickerProps) {
  const { h, s, l } = parseHSL(value);
  const [hue, setHue] = useState(h);
  const [saturation, setSaturation] = useState(s);
  const [lightness, setLightness] = useState(l);
  const [isCustomMode, setIsCustomMode] = useState(false);

  const handleHueChange = useCallback((newHue: number) => {
    setHue(newHue);
    onChange(formatHSL(newHue, saturation, lightness));
  }, [saturation, lightness, onChange]);

  const handleSaturationChange = useCallback((newSat: number) => {
    setSaturation(newSat);
    onChange(formatHSL(hue, newSat, lightness));
  }, [hue, lightness, onChange]);

  const handleLightnessChange = useCallback((newLight: number) => {
    setLightness(newLight);
    onChange(formatHSL(hue, saturation, newLight));
  }, [hue, saturation, onChange]);

  const handlePresetClick = useCallback((presetValue: string) => {
    const parsed = parseHSL(presetValue);
    setHue(parsed.h);
    setSaturation(parsed.s);
    setLightness(parsed.l);
    onChange(presetValue);
    setIsCustomMode(false);
  }, [onChange]);

  // Check if current value matches a preset
  const matchingPreset = presets.find((p) => {
    const parsed = parseHSL(p.value);
    return Math.abs(parsed.h - hue) < 5 &&
           Math.abs(parsed.s - saturation) < 5 &&
           Math.abs(parsed.l - lightness) < 5;
  });

  return (
    <div className={cn('space-y-4', className)}>
      {label && (
        <label className="block text-sm font-medium text-foreground">{label}</label>
      )}

      {/* Preset Colors Grid */}
      <div className="space-y-2">
        <p className="text-xs text-muted-foreground font-medium uppercase tracking-wide">
          Presets
        </p>
        <div className="grid grid-cols-8 gap-2">
          {presets.map((preset) => {
            const isSelected = matchingPreset?.value === preset.value && !isCustomMode;
            return (
              <button
                key={preset.value}
                type="button"
                onClick={() => handlePresetClick(preset.value)}
                className={cn(
                  'w-8 h-8 rounded-lg transition-all relative group',
                  'ring-offset-background focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2',
                  isSelected && 'ring-2 ring-primary ring-offset-2'
                )}
                style={{ backgroundColor: `hsl(${preset.value})` }}
                title={preset.name}
              >
                {isSelected && (
                  <Check className="w-4 h-4 text-white absolute inset-0 m-auto drop-shadow-md" />
                )}
                <span className="absolute -bottom-5 left-1/2 -translate-x-1/2 text-[10px] text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap">
                  {preset.name}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* Custom Color Section */}
      <div className="space-y-3 pt-3 border-t border-border">
        <button
          type="button"
          onClick={() => setIsCustomMode(!isCustomMode)}
          className={cn(
            'flex items-center gap-2 text-sm font-medium transition-colors',
            isCustomMode ? 'text-primary' : 'text-muted-foreground hover:text-foreground'
          )}
        >
          <Pipette className="w-4 h-4" />
          Custom Color
        </button>

        {isCustomMode && (
          <div className="space-y-4 animate-scale-in p-4 bg-secondary/50 rounded-xl">
            {/* Preview */}
            <div className="flex items-center gap-4">
              <div
                className="w-14 h-14 rounded-xl shadow-soft border border-border"
                style={{ backgroundColor: `hsl(${formatHSL(hue, saturation, lightness)})` }}
              />
              <div className="flex-1 text-sm">
                <p className="font-medium text-foreground">Preview</p>
                <p className="text-xs text-muted-foreground font-mono mt-0.5">
                  hsl({Math.round(hue)}, {Math.round(saturation)}%, {Math.round(lightness)}%)
                </p>
              </div>
            </div>

            {/* Hue Slider */}
            <div className="space-y-1.5">
              <div className="flex justify-between text-xs">
                <span className="text-muted-foreground">Hue</span>
                <span className="text-foreground font-medium">{Math.round(hue)}°</span>
              </div>
              <input
                type="range"
                min="0"
                max="360"
                value={hue}
                onChange={(e) => handleHueChange(Number(e.target.value))}
                className="w-full h-3 rounded-full appearance-none cursor-pointer"
                style={{
                  background: `linear-gradient(to right,
                    hsl(0, 100%, 50%),
                    hsl(60, 100%, 50%),
                    hsl(120, 100%, 50%),
                    hsl(180, 100%, 50%),
                    hsl(240, 100%, 50%),
                    hsl(300, 100%, 50%),
                    hsl(360, 100%, 50%)
                  )`,
                }}
              />
            </div>

            {/* Saturation Slider */}
            <div className="space-y-1.5">
              <div className="flex justify-between text-xs">
                <span className="text-muted-foreground">Saturation</span>
                <span className="text-foreground font-medium">{Math.round(saturation)}%</span>
              </div>
              <input
                type="range"
                min="0"
                max="100"
                value={saturation}
                onChange={(e) => handleSaturationChange(Number(e.target.value))}
                className="w-full h-3 rounded-full appearance-none cursor-pointer"
                style={{
                  background: `linear-gradient(to right,
                    hsl(${hue}, 0%, ${lightness}%),
                    hsl(${hue}, 100%, ${lightness}%)
                  )`,
                }}
              />
            </div>

            {/* Lightness Slider */}
            <div className="space-y-1.5">
              <div className="flex justify-between text-xs">
                <span className="text-muted-foreground">Lightness</span>
                <span className="text-foreground font-medium">{Math.round(lightness)}%</span>
              </div>
              <input
                type="range"
                min="20"
                max="70"
                value={lightness}
                onChange={(e) => handleLightnessChange(Number(e.target.value))}
                className="w-full h-3 rounded-full appearance-none cursor-pointer"
                style={{
                  background: `linear-gradient(to right,
                    hsl(${hue}, ${saturation}%, 20%),
                    hsl(${hue}, ${saturation}%, 50%),
                    hsl(${hue}, ${saturation}%, 70%)
                  )`,
                }}
              />
            </div>
          </div>
        )}
      </div>

      {/* Current Color Display */}
      <div className="flex items-center gap-3 p-3 bg-secondary/50 rounded-lg">
        <div
          className="w-10 h-10 rounded-lg shadow-sm border border-border"
          style={{ backgroundColor: `hsl(${value})` }}
        />
        <div className="flex-1">
          <p className="text-sm font-medium text-foreground">
            {matchingPreset && !isCustomMode ? matchingPreset.name : 'Custom Color'}
          </p>
          <p className="text-xs text-muted-foreground font-mono">
            hsl({Math.round(hue)}, {Math.round(saturation)}%, {Math.round(lightness)}%)
          </p>
        </div>
      </div>
    </div>
  );
}

// Simplified inline color swatch for forms
interface ColorSwatchProps {
  value: string;
  onChange: (value: string) => void;
  colors?: { name: string; value: string }[];
  className?: string;
}

export function ColorSwatch({
  value,
  onChange,
  colors = defaultPresets.slice(0, 8),
  className,
}: ColorSwatchProps) {
  return (
    <div className={cn('flex gap-1.5 flex-wrap', className)}>
      {colors.map((color) => {
        const isSelected = value === color.value;
        return (
          <button
            key={color.value}
            type="button"
            onClick={() => onChange(color.value)}
            className={cn(
              'w-7 h-7 rounded-md transition-all',
              'focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-1',
              isSelected && 'ring-2 ring-primary ring-offset-1'
            )}
            style={{ backgroundColor: `hsl(${color.value})` }}
            title={color.name}
          >
            {isSelected && (
              <Check className="w-3.5 h-3.5 text-white m-auto drop-shadow" />
            )}
          </button>
        );
      })}
    </div>
  );
}
