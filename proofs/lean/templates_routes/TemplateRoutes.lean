namespace Terlan.TemplateRoutes

inductive ValueType where
  | int
  | string
  | response
  deriving DecidableEq

structure RouteContract where
  pathFields : List ValueType
  valueFields : List ValueType
  result : ValueType
  deriving DecidableEq

structure HandlerContract where
  arguments : List ValueType
  result : ValueType
  deriving DecidableEq

def lowerRoute (route : RouteContract) : HandlerContract where
  arguments := route.pathFields ++ route.valueFields
  result := route.result

theorem loweringPreservesRouteArity (route : RouteContract) :
    (lowerRoute route).arguments.length =
      route.pathFields.length + route.valueFields.length := by
  simp [lowerRoute]

theorem loweringPreservesRouteShape (route : RouteContract) :
    (lowerRoute route).arguments = route.pathFields ++ route.valueFields := by
  rfl

theorem loweringPreservesReturnType (route : RouteContract) :
    (lowerRoute route).result = route.result := by
  rfl

inductive TemplateSlot where
  | record : String -> ValueType -> TemplateSlot
  | shape : String -> ValueType -> TemplateSlot
  | value : ValueType -> TemplateSlot
  | nested : TemplateSlot -> TemplateSlot
  deriving DecidableEq

def slotTyped : TemplateSlot -> Bool
  | .record name _ => !name.isEmpty
  | .shape name _ => !name.isEmpty
  | .value _ => true
  | .nested slot => slotTyped slot

theorem typedNestedInterpolationAccepted :
    slotTyped (.nested (.record "user" .string)) = true := by
  decide

theorem emptyRecordSlotRejected :
    slotTyped (.record "" .string) = false := by
  decide

def routeShapeMatches (route : RouteContract) (handler : HandlerContract) : Bool :=
  route.pathFields ++ route.valueFields == handler.arguments &&
    route.result == handler.result

theorem mismatchedRouteHandlerRejected :
    routeShapeMatches
      { pathFields := [.int], valueFields := [], result := .response }
      { arguments := [.string], result := .response } = false := by
  rfl

end Terlan.TemplateRoutes
